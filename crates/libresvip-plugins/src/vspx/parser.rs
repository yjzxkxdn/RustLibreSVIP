//! Port of `libresvip.plugins.vspx.vspx_parser`.

use libresvip_core::core::tick_counter::shift_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack as ProjectInstrumentalTrack, Note, ParamCurve, Project, SingingTrack,
    SongTempo, TimeSignature, Track as ProjectTrack,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::model::{
    Beat, InstrumentalTrack, NoteTrack, ParamKind, Tempo, Track, Trill, VspxProject,
};
use super::pitch_curve::BasePitchCurve;

/// `InputOptions(EnableInstrumentalTrackImportationMixin,
/// EnablePitchImportationMixin)`.
#[derive(Clone, Debug)]
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
}

pub struct VocalSharpParser {
    options: InputOptions,
    default_trill: Option<Trill>,
    synchronizer: TimeSynchronizer,
    first_bar_length: i64,
}

impl VocalSharpParser {
    pub fn new(options: &Options) -> Self {
        Self {
            options: InputOptions::from_options(options),
            default_trill: None,
            synchronizer: TimeSynchronizer::from_project(&[]),
            first_bar_length: 0,
        }
    }

    pub fn parse_project(&mut self, vspx_project: &VspxProject) -> Result<Project> {
        self.default_trill = vspx_project.project.default_trill;
        let time_signatures = Self::parse_time_signatures(&vspx_project.project.beats);
        let Some(first_time_signature) = time_signatures.first() else {
            // Python raises IndexError on `time_signatures[0]`.
            return Err(LibreSvipError::msg("time signature list is empty"));
        };
        self.first_bar_length = first_time_signature.bar_length().round_ties_even() as i64;
        let tempos = self.parse_tempos(&vspx_project.project.tempos);
        self.synchronizer = TimeSynchronizer::from_project(&tempos);
        let mut track_list = self.parse_singing_tracks(&vspx_project.project.tracks)?;
        track_list.extend(self.parse_instrumental_tracks(&vspx_project.project.tracks));
        Ok(Project {
            version: String::new(),
            song_tempo_list: tempos,
            time_signature_list: time_signatures,
            track_list,
        })
    }

    fn parse_time_signatures(beat_list: &[Beat]) -> Vec<TimeSignature> {
        beat_list
            .iter()
            .map(|beat| {
                TimeSignature::new(beat.bar_index, beat.beat_per_bar, beat.bar_divide)
            })
            .collect()
    }

    fn parse_tempos(&self, tempo_list: &[Tempo]) -> Vec<SongTempo> {
        let tempos: Vec<SongTempo> = tempo_list
            .iter()
            .map(|tempo| SongTempo::new(tempo.pos, tempo.bpm))
            .collect();
        shift_tempo_list(&tempos, self.first_bar_length)
    }

    fn parse_singing_tracks(&mut self, track_list: &[Track]) -> Result<Vec<ProjectTrack>> {
        let mut tracks = Vec::new();
        for track in track_list {
            let Track::Note(track) = track else {
                continue;
            };
            let mut singing_track = SingingTrack {
                title: track.name.clone().unwrap_or_default(),
                mute: track.is_muted(),
                solo: track.is_soloed(),
                ai_singer_name: track.singer.clone().unwrap_or_default(),
                note_list: Self::parse_notes(&track.notes)?,
                ..Default::default()
            };
            if self.options.import_pitch {
                singing_track.edited_params.pitch = self.parse_pitch(track)?;
            }
            tracks.push(ProjectTrack::Singing(singing_track));
        }
        Ok(tracks)
    }

    fn parse_notes(note_list: &[super::model::Note]) -> Result<Vec<Note>> {
        let mut notes = Vec::with_capacity(note_list.len());
        for note in note_list {
            let key_number = note
                .key_number()
                .map_err(|error| LibreSvipError::msg(error))?;
            notes.push(Note {
                start_pos: note.pos,
                length: note.duration,
                key_number,
                lyric: note.lyric.clone().unwrap_or_default(),
                ..Default::default()
            });
        }
        Ok(notes)
    }

    fn parse_pitch(&self, note_track: &NoteTrack) -> Result<ParamCurve> {
        let mut base_pitch_curve = BasePitchCurve::new(
            note_track,
            self.default_trill.as_ref(),
            self.synchronizer.clone(),
        )
        .map_err(LibreSvipError::msg)?;
        let mut pitch_points: Vec<Point> = vec![Point::start_point()];
        let mut prev_tick: Option<i64> = None;
        for param_point in &note_track.parameter {
            if param_point.kind != ParamKind::Pit {
                continue;
            }
            let cur_tick = param_point.time + self.first_bar_length;
            match prev_tick {
                None => pitch_points.push(Point::new(cur_tick, -100)),
                Some(prev_tick) => {
                    // Faithful to the Python quirk: the comparison mixes the raw
                    // parameter time with the already shifted previous tick.
                    if param_point.time - prev_tick > 1 {
                        pitch_points.push(Point::new(prev_tick, -100));
                        pitch_points.push(Point::new(cur_tick, -100));
                    }
                }
            }
            let secs = self
                .synchronizer
                .get_actual_secs_from_ticks(param_point.time);
            if let Some(base_key) = base_pitch_curve.semitone_value_at(secs) {
                pitch_points.push(Point::new(
                    cur_tick,
                    (base_key * 100.0 + param_point.value as f64).round_ties_even() as i64,
                ));
            }
            prev_tick = Some(cur_tick);
        }
        if pitch_points.len() > 1 {
            let last_x = pitch_points
                .last()
                .map(|point| point.x)
                .unwrap_or_default();
            pitch_points.push(Point::new(last_x, -100));
        }
        pitch_points.push(Point::end_point());
        if pitch_points.len() <= 2 {
            pitch_points.clear();
        }
        Ok(ParamCurve::new(pitch_points))
    }

    fn parse_instrumental_tracks(&self, track_list: &[Track]) -> Vec<ProjectTrack> {
        let mut tracks = Vec::new();
        if !self.options.import_instrumental_track {
            return tracks;
        }
        for track in track_list {
            let instrumental: &InstrumentalTrack = match track {
                Track::Stereo(track) => track,
                Track::Mono(track) => track,
                Track::Note(_) => continue,
            };
            let track_name = instrumental.name.clone().unwrap_or_default();
            for (index, sequence) in instrumental.sequences.iter().enumerate() {
                let title = sequence
                    .name
                    .clone()
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("{track_name} {}", index + 1));
                tracks.push(ProjectTrack::Instrumental(ProjectInstrumentalTrack {
                    title,
                    mute: instrumental.is_muted(),
                    solo: instrumental.is_soloed(),
                    offset: sequence.pos.unwrap_or(0),
                    audio_file_path: sequence.path.clone().unwrap_or_default(),
                    ..Default::default()
                }));
            }
        }
        tracks
    }
}
