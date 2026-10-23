//! Port of `libresvip.plugins.s5p.synthv_editor_parser`.

use std::path::Path;

use libresvip_core::core::tick_counter::shift_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature,
    Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use crate::s5p::math_utils::{db_to_float, linear_interpolation, py_round, ratio_to_db};
use crate::s5p::model::{
    S5pDbDefaults, S5pInstrumental, S5pMeterItem, S5pMixer, S5pNote, S5pParameters, S5pProject,
    S5pTempoItem, S5pTrack, TICK_RATE,
};
use crate::s5p::synthv_pitch::{NoteStruct, SynthVPitchSimulator};

#[derive(Clone, Copy, Debug)]
pub struct InputOptions {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
}

impl InputOptions {
    pub fn from_options(options: &Options) -> Self {
        Self {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
        }
    }

    pub fn from_schema_defaults() -> Self {
        Self {
            import_instrumental_track: true,
            import_pitch: true,
        }
    }
}

impl Default for InputOptions {
    fn default() -> Self {
        Self::from_schema_defaults()
    }
}

pub struct SynthVEditorParser {
    options: InputOptions,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
    time_signatures: Vec<TimeSignature>,
}

impl SynthVEditorParser {
    pub fn new(options: InputOptions) -> Self {
        Self {
            options,
            first_bar_length: 0,
            synchronizer: TimeSynchronizer::from_project(&[]),
            time_signatures: Vec::new(),
        }
    }

    pub fn parse_project(&mut self, s5p_project: &S5pProject) -> Result<Project> {
        self.time_signatures = Self::parse_time_signatures(&s5p_project.meter);
        self.first_bar_length = py_round(self.time_signatures[0].bar_length());
        let tempo_list = self.parse_tempos(&s5p_project.tempo);
        self.synchronizer = TimeSynchronizer::from_project(&tempo_list);
        let mut track_list = self.parse_singing_tracks(&s5p_project.tracks)?;
        if self.options.import_instrumental_track && !s5p_project.instrumental.filename.is_empty() {
            track_list.push(Track::Instrumental(
                self.parse_instrumental_track(&s5p_project.instrumental, &s5p_project.mixer)?,
            ));
        }
        Ok(Project {
            version: String::new(),
            song_tempo_list: tempo_list,
            time_signature_list: self.time_signatures.clone(),
            track_list,
        })
    }

    pub fn parse_time_signatures(meter: &[S5pMeterItem]) -> Vec<TimeSignature> {
        let mut time_signatures: Vec<TimeSignature> = meter
            .iter()
            .map(|item| {
                TimeSignature::new(
                    item.measure,
                    item.beat_per_measure,
                    item.beat_granularity,
                )
            })
            .collect();
        if time_signatures.is_empty() {
            time_signatures.push(TimeSignature::new(0, 4, 4));
        }
        time_signatures
    }

    fn parse_tempos(&self, tempo: &[S5pTempoItem]) -> Vec<SongTempo> {
        let tempos: Vec<SongTempo> = tempo
            .iter()
            .map(|item| {
                SongTempo::new(
                    item.position.div_euclid(TICK_RATE),
                    item.beat_per_minute,
                )
            })
            .collect();
        shift_tempo_list(&tempos, self.first_bar_length)
    }

    fn parse_singing_tracks(&self, tracks: &[S5pTrack]) -> Result<Vec<Track>> {
        let mut singing_tracks: Vec<Track> = Vec::new();
        for (index, track) in tracks.iter().enumerate() {
            let note_list = Self::parse_notes(&track.notes, &track.db_defaults);
            if note_list.is_empty() {
                continue;
            }
            if self.synchronizer.tempo_list.is_empty() {
                return Err(LibreSvipError::msg(
                    "cannot convert notes without a tempo list",
                ));
            }
            let s5p_notes: Vec<&S5pNote> = track.notes.iter().filter_map(|note| note.as_ref()).collect();
            let edited_params =
                self.parse_params(&track.parameters, &s5p_notes, &track.db_defaults);
            let title = if track.name.is_empty() {
                format!("Track {}", index + 1)
            } else {
                track.name.clone()
            };
            singing_tracks.push(Track::Singing(SingingTrack {
                title,
                mute: track.mixer.muted,
                solo: track.mixer.solo,
                volume: Self::parse_volume(track.mixer.gain_decibel),
                pan: track.mixer.pan,
                ai_singer_name: track.db_name.clone(),
                note_list,
                edited_params,
                ..SingingTrack::default()
            }));
        }
        Ok(singing_tracks)
    }

    pub fn parse_volume(gain: f64) -> f64 {
        if gain >= 0.0 {
            (gain / ratio_to_db(4.0) + 1.0).min(2.0)
        } else {
            db_to_float(gain)
        }
    }

    fn parse_instrumental_track(
        &self,
        track: &S5pInstrumental,
        mixer: &S5pMixer,
    ) -> Result<InstrumentalTrack> {
        if self.synchronizer.tempo_list.is_empty() {
            return Err(LibreSvipError::msg(
                "cannot convert an instrumental track without a tempo list",
            ));
        }
        let title = Path::new(&track.filename)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(InstrumentalTrack {
            title,
            mute: mixer.instrumental_muted,
            volume: Self::parse_volume(mixer.gain_instrumental_decibel),
            audio_file_path: track.filename.clone(),
            offset: py_round(self.synchronizer.get_actual_ticks_from_secs(track.offset)),
            ..InstrumentalTrack::default()
        })
    }

    fn parse_notes(s5p_notes: &[Option<S5pNote>], db_defaults: &S5pDbDefaults) -> Vec<Note> {
        let mut notes = Vec::new();
        for s5p_note in s5p_notes.iter().flatten() {
            let stripped = s5p_note.lyric.trim_start_matches('.').replace(' ', "");
            let lyric = if stripped.is_empty() {
                db_defaults.lyric.clone().unwrap_or_default()
            } else {
                stripped
            };
            let pronunciation = if s5p_note.lyric.starts_with('.') && !s5p_note.comment.is_empty() {
                Some(s5p_note.comment.clone())
            } else {
                None
            };
            notes.push(Note {
                start_pos: py_round(s5p_note.onset as f64 / TICK_RATE as f64),
                length: py_round(s5p_note.duration as f64 / TICK_RATE as f64),
                key_number: s5p_note.pitch,
                lyric,
                pronunciation,
                ..Note::default()
            });
        }
        notes
    }

    fn parse_params(
        &self,
        parameters: &S5pParameters,
        note_list: &[&S5pNote],
        db_defaults: &S5pDbDefaults,
    ) -> Params {
        let mut params = Params::default();
        if self.options.import_pitch {
            let note_structs: Vec<NoteStruct> = note_list
                .iter()
                .map(|note| self.note_to_note_struct(note, db_defaults))
                .collect();
            let pitch_simulator =
                SynthVPitchSimulator::new(self.synchronizer.clone(), &note_structs);
            params.pitch =
                self.parse_pitch_curve(parameters, &pitch_simulator, &note_structs);
        }
        params
    }

    fn parse_pitch_curve(
        &self,
        parameters: &S5pParameters,
        pitch_simulator: &SynthVPitchSimulator,
        note_structs: &[NoteStruct],
    ) -> ParamCurve {
        let scale = parameters.interval as f64 / TICK_RATE as f64;
        let mut relative_point_groups: Vec<Vec<Point>> = Vec::new();
        for group in split_when(&parameters.pitch_delta.0, |p1, p2| {
            p2.offset > p1.offset + 1.0
        }) {
            if group.len() > 1 {
                relative_point_groups.push(
                    group
                        .iter()
                        .map(|s5p_point| {
                            Point::new(
                                py_round(s5p_point.offset * scale),
                                py_round(s5p_point.value),
                            )
                        })
                        .collect(),
                );
            }
        }
        let mut points = vec![Point::start_point()];
        for (start_secs, end_secs) in Self::iter_note_segments(note_structs) {
            let start_ticks = py_round(self.synchronizer.get_actual_ticks_from_secs(start_secs));
            let end_ticks = py_round(self.synchronizer.get_actual_ticks_from_secs(end_secs));
            points.push(Point::new(
                start_ticks.saturating_add(self.first_bar_length),
                -100,
            ));
            let mut ticks = start_ticks;
            while ticks < end_ticks {
                points.push(Point::new(
                    ticks.saturating_add(self.first_bar_length),
                    py_round(
                        pitch_simulator.pitch_at_ticks(ticks)
                            + Self::relative_pitch_at_ticks(ticks, &relative_point_groups),
                    ),
                ));
                match ticks.checked_add(5) {
                    Some(next) => ticks = next,
                    None => break,
                }
            }
            points.push(Point::new(
                end_ticks.saturating_add(self.first_bar_length),
                py_round(
                    pitch_simulator.pitch_at_ticks(end_ticks)
                        + Self::relative_pitch_at_ticks(end_ticks, &relative_point_groups),
                ),
            ));
            points.push(Point::new(
                end_ticks.saturating_add(self.first_bar_length),
                -100,
            ));
        }
        points.push(Point::end_point());
        ParamCurve::new(points)
    }

    pub fn iter_note_segments(note_structs: &[NoteStruct]) -> Vec<(f64, f64)> {
        if note_structs.is_empty() {
            return Vec::new();
        }
        let mut segments: Vec<(f64, f64)> = Vec::new();
        let mut start_secs = note_structs[0].start;
        let mut end_secs = note_structs[0].end;
        for note in &note_structs[1..] {
            if note.start - end_secs > 0.01 {
                segments.push((start_secs, end_secs));
                start_secs = note.start;
            }
            end_secs = note.end;
        }
        segments.push((start_secs, end_secs));
        segments
    }

    pub fn relative_pitch_at_ticks(ticks: i64, relative_point_groups: &[Vec<Point>]) -> f64 {
        for points in relative_point_groups {
            if points.is_empty() {
                continue;
            }
            let first = points[0];
            let last = points[points.len() - 1];
            if ticks < first.x || ticks > last.x {
                continue;
            }
            if points.len() == 1 {
                return first.y as f64;
            }
            if ticks <= first.x {
                return first.y as f64;
            }
            if ticks >= last.x {
                return last.y as f64;
            }
            for pair in points.windows(2) {
                let prev_point = pair[0];
                let point = pair[1];
                if prev_point.x <= ticks && ticks <= point.x {
                    return linear_interpolation(
                        ticks,
                        (prev_point.x, prev_point.y),
                        (point.x, point.y),
                    );
                }
            }
        }
        0.0
    }

    fn note_to_note_struct(&self, note: &S5pNote, db_defaults: &S5pDbDefaults) -> NoteStruct {
        let onset = py_round(note.onset as f64 / TICK_RATE as f64);
        let end_pos =
            onset.saturating_add(py_round(note.duration as f64 / TICK_RATE as f64));
        NoteStruct {
            key: note.pitch,
            start: self.synchronizer.get_actual_secs_from_ticks(onset),
            end: self.synchronizer.get_actual_secs_from_ticks(end_pos),
            portamento_offset: note.t_f0_offset.unwrap_or(0.0),
            portamento_left: note.t_f0_left.unwrap_or(db_defaults.t_f0_left),
            portamento_right: note.t_f0_right.unwrap_or(db_defaults.t_f0_right),
            depth_left: note.d_f0_left.unwrap_or(db_defaults.d_f0_left),
            depth_right: note.d_f0_right.unwrap_or(db_defaults.d_f0_right),
            vibrato_start: note.t_f0_vbr_start.unwrap_or(db_defaults.t_f0_vbr_start),
            vibrato_left: note.t_f0_vbr_left.unwrap_or(db_defaults.t_f0_vbr_left),
            vibrato_right: note.t_f0_vbr_right.unwrap_or(db_defaults.t_f0_vbr_right),
            vibrato_depth: note.d_f0_vbr.unwrap_or(db_defaults.d_f0_vbr) * 40.0,
            vibrato_frequency: note.f_f0_vbr.unwrap_or(db_defaults.f_f0_vbr),
            vibrato_phase: std::f64::consts::PI * note.p_f0_vbr.unwrap_or(db_defaults.p_f0_vbr),
        }
    }
}

/// `more_itertools.split_when`: yield groups split before an element for which
/// `predicate(previous, current)` is true.
fn split_when<T: Copy>(items: &[T], predicate: impl Fn(T, T) -> bool) -> Vec<Vec<T>> {
    let mut groups: Vec<Vec<T>> = Vec::new();
    let mut current: Vec<T> = Vec::new();
    for &item in items {
        if let Some(&previous) = current.last() {
            if predicate(previous, item) {
                groups.push(std::mem::take(&mut current));
            }
        }
        current.push(item);
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}
