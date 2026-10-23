//! Port of `libresvip.plugins.s5p.synthv_editor_generator`.

use libresvip_core::core::tick_counter::skip_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Params, Project, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use crate::s5p::math_utils::{py_round, ratio_to_db};
use crate::s5p::model::{
    S5pDbDefaults, S5pInstrumental, S5pMeterItem, S5pMixer, S5pNote, S5pParameters, S5pPoint,
    S5pPoints, S5pProject, S5pTempoItem, S5pTrack, S5pTrackMixer, TICK_RATE,
};
use crate::s5p::synthv_pitch::{NoteStruct, SynthVPitchSimulator};

#[derive(Clone, Copy, Debug, Default)]
pub struct OutputOptions;

impl OutputOptions {
    pub fn from_options(_options: &Options) -> Self {
        Self
    }
}

pub struct SynthVEditorGenerator {
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
    time_signatures: Vec<TimeSignature>,
}

impl SynthVEditorGenerator {
    pub fn new(_options: OutputOptions) -> Self {
        Self {
            first_bar_length: 0,
            synchronizer: TimeSynchronizer::from_project(&[]),
            time_signatures: Vec::new(),
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<S5pProject> {
        let first_signature = project
            .time_signature_list
            .first()
            .ok_or_else(|| LibreSvipError::msg("time_signature_list must not be empty"))?;
        self.first_bar_length = py_round(first_signature.bar_length());
        self.time_signatures = project.time_signature_list.clone();
        let tempo = self.generate_tempos(&project.song_tempo_list);
        let meter = Self::generate_time_signatures(&project.time_signature_list);
        let tracks = self.generate_singing_tracks(&project.track_list)?;
        let mut s5p_project = S5pProject {
            tempo,
            meter,
            tracks,
            ..S5pProject::default()
        };
        if let Some(track) = project
            .track_list
            .iter()
            .find_map(Track::as_instrumental)
        {
            let (instrumental, mixer) = self.generate_instrumental_track_and_mixer(track)?;
            s5p_project.instrumental = instrumental;
            s5p_project.mixer = mixer;
        }
        Ok(s5p_project)
    }

    fn generate_tempos(&mut self, song_tempo_list: &[SongTempo]) -> Vec<S5pTempoItem> {
        let song_tempo_list = skip_tempo_list(song_tempo_list, self.first_bar_length);
        self.synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
        song_tempo_list
            .iter()
            .map(|song_tempo| S5pTempoItem {
                position: song_tempo.position.saturating_mul(TICK_RATE),
                beat_per_minute: song_tempo.bpm,
            })
            .collect()
    }

    pub fn generate_time_signatures(
        time_signature_list: &[TimeSignature],
    ) -> Vec<S5pMeterItem> {
        time_signature_list
            .iter()
            .map(|time_signature| S5pMeterItem {
                measure: time_signature.bar_index,
                beat_per_measure: time_signature.numerator,
                beat_granularity: time_signature.denominator,
            })
            .collect()
    }

    pub fn generate_volume(volume: f64) -> f64 {
        if volume > 0.0 {
            ratio_to_db(volume.max(0.01)).max(-70.0)
        } else {
            -70.0
        }
    }

    fn generate_singing_tracks(&self, track_list: &[Track]) -> Result<Vec<S5pTrack>> {
        let mut tracks = Vec::new();
        for (index, track) in track_list.iter().enumerate() {
            if let Track::Singing(track) = track {
                let s5p_notes = Self::generate_notes(&track.note_list);
                let track_mixer = S5pTrackMixer {
                    solo: track.solo,
                    muted: track.mute,
                    pan: track.pan,
                    gain_decibel: Self::generate_volume(track.volume),
                    ..S5pTrackMixer::default()
                };
                let mut s5p_track = S5pTrack {
                    display_order: index as i64,
                    name: track.title.clone(),
                    db_name: track.ai_singer_name.clone(),
                    notes: s5p_notes.iter().cloned().map(Some).collect(),
                    mixer: track_mixer,
                    ..S5pTrack::default()
                };
                let db_defaults = s5p_track.db_defaults.clone();
                s5p_track.parameters =
                    self.generate_parameters(&track.edited_params, &s5p_notes, &db_defaults)?;
                tracks.push(s5p_track);
            }
        }
        if tracks.is_empty() {
            tracks.push(S5pTrack {
                name: "Unnamed Track".to_owned(),
                notes: vec![None],
                ..S5pTrack::default()
            });
        }
        Ok(tracks)
    }

    fn generate_notes(note_list: &[Note]) -> Vec<S5pNote> {
        note_list
            .iter()
            .map(|note| S5pNote {
                lyric: note.lyric.clone(),
                onset: note.start_pos.saturating_mul(TICK_RATE),
                duration: note.length.saturating_mul(TICK_RATE),
                pitch: note.key_number,
                ..S5pNote::default()
            })
            .collect()
    }

    fn generate_instrumental_track_and_mixer(
        &self,
        track: &InstrumentalTrack,
    ) -> Result<(S5pInstrumental, S5pMixer)> {
        if self.synchronizer.tempo_list.is_empty() {
            return Err(LibreSvipError::msg(
                "cannot convert an instrumental track without a tempo list",
            ));
        }
        Ok((
            S5pInstrumental {
                filename: track.audio_file_path.clone(),
                offset: self
                    .synchronizer
                    .get_actual_secs_from_ticks(track.offset),
            },
            S5pMixer {
                gain_instrumental_decibel: Self::generate_volume(track.volume),
                instrumental_muted: track.mute,
                ..S5pMixer::default()
            },
        ))
    }

    fn generate_parameters(
        &self,
        edited_params: &Params,
        note_list: &[S5pNote],
        db_defaults: &S5pDbDefaults,
    ) -> Result<S5pParameters> {
        let interval = py_round(TICK_RATE as f64 * 3.75);
        let note_structs: Vec<NoteStruct> = note_list
            .iter()
            .map(|note| self.note_to_note_struct(note, db_defaults))
            .collect();
        let pitch_simulator =
            SynthVPitchSimulator::new(self.synchronizer.clone(), &note_structs);
        Ok(S5pParameters {
            interval,
            pitch_delta: self.generate_pitch_delta(
                &edited_params.pitch.points,
                &pitch_simulator,
                interval,
            ),
            ..S5pParameters::default()
        })
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

    fn generate_pitch_delta(
        &self,
        pitch: &[Point],
        pitch_simulator: &SynthVPitchSimulator,
        interval: i64,
    ) -> S5pPoints {
        let scale = interval as f64 / TICK_RATE as f64;
        let relative_points: Vec<S5pPoint> = pitch
            .iter()
            .filter(|point| {
                point.y != -100
                    && self.first_bar_length <= point.x
                    && point.x < i64::MAX / 2
            })
            .map(|point| S5pPoint::new(
                py_round((point.x - self.first_bar_length) as f64 / scale) as f64,
                point.y as f64
                    - pitch_simulator.pitch_at_ticks(point.x - self.first_bar_length),
            ))
            .collect();
        S5pPoints::from_points(Self::compress_pitch_delta(&relative_points))
    }

    pub fn compress_pitch_delta(relative_points: &[S5pPoint]) -> Vec<S5pPoint> {
        if relative_points.is_empty() {
            return Vec::new();
        }
        let mut compressed_points: Vec<S5pPoint> = Vec::new();
        let tolerance = 1e-3;
        for group in split_when(relative_points, |p1, p2| p2.offset > p1.offset + 1.0) {
            if group.is_empty() {
                continue;
            }
            let nonzero_indexes: Vec<usize> = group
                .iter()
                .enumerate()
                .filter(|(_, point)| point.value.abs() > tolerance)
                .map(|(index, _)| index)
                .collect();
            let Some(first) = nonzero_indexes.first().copied() else {
                continue;
            };
            let last = *nonzero_indexes.last().unwrap_or(&first);
            if group.len() == 1 {
                compressed_points.push(group[0]);
                compressed_points.push(group[0]);
                continue;
            }
            let start_index = first.saturating_sub(1);
            let end_index = (last + 1).min(group.len() - 1);
            compressed_points.extend_from_slice(&group[start_index..=end_index]);
        }
        compressed_points
    }
}

/// `more_itertools.split_when` for a slice.
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
