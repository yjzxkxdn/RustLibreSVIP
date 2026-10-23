//! Port of `libresvip.plugins.vspx.vspx_generator`.

use libresvip_core::core::tick_counter::skip_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Project, SongTempo, Track as ProjectTrack};
use libresvip_core::utils::audio::audio_track_info;

use super::model::{
    Beat, InstrumentalTrack, Note, NoteTrack, ParamPoint, Sequence, Syllable, Tempo, Track,
    VspxProject,
};
use super::music_math::{linear_interpolation, midi2note};
use super::pitch_curve::BasePitchCurve;

/// `OutputOptions(BaseModel)` — no fields.
#[derive(Clone, Debug, Default)]
pub struct OutputOptions;

impl OutputOptions {
    pub fn from_options(_options: &libresvip_core::options::Options) -> Self {
        Self
    }
}

/// `str(bool)` — Python spells booleans with a capital letter.
pub fn py_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}

pub struct VocalSharpGenerator {
    #[allow(dead_code)]
    options: OutputOptions,
    synchronizer: TimeSynchronizer,
    first_bar_length: i64,
}

impl VocalSharpGenerator {
    pub fn new(options: OutputOptions) -> Self {
        Self {
            options,
            synchronizer: TimeSynchronizer::from_project(&[]),
            first_bar_length: 0,
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<VspxProject> {
        let mut vspx_project = VspxProject::default();
        let Some(first_time_signature) = project.time_signature_list.first() else {
            // Python raises IndexError on `time_signature_list[0]`.
            return Err(LibreSvipError::msg("time signature list is empty"));
        };
        self.first_bar_length = first_time_signature.bar_length().round_ties_even() as i64;
        self.synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        vspx_project.project.tempos = self.generate_tempos(&project.song_tempo_list);
        vspx_project.project.beats = Self::generate_time_signatures(&project.time_signature_list);
        let singing_tracks = self.generate_singing_tracks(&project.track_list)?;
        vspx_project
            .project
            .tracks
            .extend(singing_tracks.iter().cloned().map(Track::Note));
        vspx_project.project.tracks.extend(
            self.generate_instrumental_tracks(&project.track_list)?
                .into_iter(),
        );
        let max_duration = singing_tracks
            .iter()
            .flat_map(|track| track.notes.iter())
            .map(|note| note.pos + note.duration)
            .max()
            .unwrap_or(0);
        let vspx_duration =
            (((max_duration - 122_880).max(0) as f64) / 30_720.0).ceil() as i64 * 30_720 + 122_880;
        vspx_project.project.duration = Some(vspx_duration);
        Ok(vspx_project)
    }

    fn generate_time_signatures(time_signatures: &[libresvip_core::model::base::TimeSignature]) -> Vec<Beat> {
        time_signatures
            .iter()
            .map(|time_signature| Beat {
                bar_index: time_signature.bar_index,
                beat_per_bar: time_signature.numerator,
                bar_divide: time_signature.denominator,
            })
            .collect()
    }

    fn generate_tempos(&self, tempos: &[SongTempo]) -> Vec<Tempo> {
        skip_tempo_list(tempos, self.first_bar_length)
            .into_iter()
            .map(|tempo| Tempo {
                pos: tempo.position,
                bpm: tempo.bpm,
            })
            .collect()
    }

    fn generate_instrumental_tracks(
        &self,
        track_list: &[ProjectTrack],
    ) -> Result<Vec<Track>> {
        let mut tracks: Vec<Track> = Vec::new();
        for track in track_list {
            let ProjectTrack::Instrumental(track) = track else {
                continue;
            };
            let Some(track_info) = audio_track_info(track.audio_file_path.as_str(), true) else {
                continue;
            };
            let sequence = Sequence {
                name: Some(track.title.clone()),
                path: Some(track.audio_file_path.clone()),
                pos: Some(track.offset),
            };
            let instrumental = InstrumentalTrack {
                name: Some(track.title.clone()),
                is_mute: Some(py_bool(track.mute).to_owned()),
                is_solo: Some(py_bool(track.solo).to_owned()),
                sequences: vec![sequence],
                ..Default::default()
            };
            if track_info.channels == 1 {
                tracks.push(Track::Mono(instrumental));
            } else if track_info.channels == 2 {
                tracks.push(Track::Stereo(instrumental));
            }
        }
        Ok(tracks)
    }

    fn generate_singing_tracks(&mut self, track_list: &[ProjectTrack]) -> Result<Vec<NoteTrack>> {
        let mut note_tracks = Vec::new();
        for track in track_list {
            let ProjectTrack::Singing(track) = track else {
                continue;
            };
            let mut note_track = NoteTrack {
                name: Some(track.title.clone()),
                singer: Some(track.ai_singer_name.clone()),
                is_mute: Some(py_bool(track.mute).to_owned()),
                is_solo: Some(py_bool(track.solo).to_owned()),
                notes: Self::generate_notes(&track.note_list),
                ..Default::default()
            };
            let pitch_points = self.generate_pitch(&track.edited_params.pitch, &note_track)?;
            if !pitch_points.is_empty() {
                note_track.parameter = pitch_points;
            }
            note_tracks.push(note_track);
        }
        Ok(note_tracks)
    }

    fn generate_notes(notes: &[libresvip_core::model::base::Note]) -> Vec<Note> {
        notes
            .iter()
            .map(|note| Note {
                pos: note.start_pos,
                pitch: midi2note(note.key_number as f64),
                duration: note.length,
                lyric: if note.lyric.is_empty() {
                    note.pronunciation.clone()
                } else {
                    Some(note.lyric.clone())
                },
                trill: None,
                syllables: vec![Syllable::default()],
            })
            .collect()
    }

    fn generate_pitch(
        &mut self,
        pitch: &libresvip_core::model::base::ParamCurve,
        note_track: &NoteTrack,
    ) -> Result<Vec<ParamPoint>> {
        let mut base_pitch_curve =
            BasePitchCurve::new(note_track, None, self.synchronizer.clone())
                .map_err(LibreSvipError::msg)?;
        let mut pitch_points: Vec<ParamPoint> = Vec::new();
        let mut prev_point: Option<ParamPoint> = None;
        for point in &pitch.points {
            let cur_tick = point.x - self.first_bar_length;
            let cur_secs = self.synchronizer.get_actual_secs_from_ticks(cur_tick);
            let base_key = if point.y > 0 {
                base_pitch_curve.semitone_value_at(cur_secs)
            } else {
                None
            };
            match base_key {
                Some(base_key) => {
                    if let Some(prev) = prev_point {
                        let cur_value = point.y as f64 - base_key * 100.0;
                        let mut tick = prev.time + 1;
                        while tick < cur_tick {
                            let value = linear_interpolation(
                                tick as f64,
                                (prev.time as f64, prev.value as f64),
                                (cur_tick as f64, cur_value),
                            );
                            pitch_points.push(ParamPoint::pit(tick, value.round_ties_even() as i64));
                            tick += 1;
                        }
                    } else {
                        pitch_points.push(ParamPoint::pit(
                            cur_tick,
                            (point.y as f64 - base_key * 100.0).round_ties_even() as i64,
                        ));
                    }
                    prev_point = pitch_points.last().copied();
                }
                None => {
                    prev_point = None;
                }
            }
        }
        Ok(pitch_points)
    }
}
