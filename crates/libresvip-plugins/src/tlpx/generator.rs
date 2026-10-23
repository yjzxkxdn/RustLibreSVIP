//! Port of `libresvip.plugins.tlpx.tunelab_generator`.

use libresvip_core::core::tick_counter::skip_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, ParamCurve, Project, SongTempo, TimeSignature, Track};
use libresvip_core::utils::audio::audio_track_info;

use super::model::{
    TuneLabAudioPart, TuneLabEditorInfo, TuneLabExportConfig, TuneLabMidiPart, TuneLabNote,
    TuneLabPart, TuneLabPoint, TuneLabProject, TuneLabTempo, TuneLabTimeSignature, TuneLabTrack,
};
use super::music::{py_round, ratio_to_db};

/// `TuneLabVersionCompatibility` (IntEnum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuneLabVersionCompatibility {
    Legacy = 0,
    Current = 1,
}

impl TuneLabVersionCompatibility {
    pub fn from_value(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Legacy),
            1 => Some(Self::Current),
            _ => None,
        }
    }

    pub fn value(self) -> i64 {
        self as i64
    }
}

#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub version_compatibility: TuneLabVersionCompatibility,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            version_compatibility: TuneLabVersionCompatibility::Current,
        }
    }
}

pub struct TuneLabGenerator {
    options: OutputOptions,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
}

impl TuneLabGenerator {
    pub fn generate_project(project: &Project, options: &OutputOptions) -> Result<TuneLabProject> {
        let Some(first_signature) = project.time_signature_list.first() else {
            return Err(LibreSvipError::Validation(
                "project has no time signatures".to_owned(),
            ));
        };
        let first_bar_length = py_round(first_signature.bar_length());
        if first_bar_length == 0 {
            // Python divides by zero here; report it instead of inventing a value.
            return Err(LibreSvipError::Validation(
                "invalid time signature: bar length is zero".to_owned(),
            ));
        }
        let mut generator = Self {
            options: options.clone(),
            first_bar_length,
            synchronizer: TimeSynchronizer::from_project(&project.song_tempo_list),
        };
        let tempo_list = generator.generate_tempos(&project.song_tempo_list);
        let time_signature_list = Self::generate_time_signatures(&project.time_signature_list);
        let tracks = generator.generate_tracks(&project.track_list);
        Ok(TuneLabProject {
            version: generator.options.version_compatibility.value(),
            tempos: tempo_list,
            time_signatures: time_signature_list,
            tracks,
            editor_info: TuneLabEditorInfo::default(),
            export_config: TuneLabExportConfig::default(),
            extras: Vec::new(),
        })
    }

    fn generate_tempos(&self, song_tempo_list: &[SongTempo]) -> Vec<TuneLabTempo> {
        skip_tempo_list(song_tempo_list, self.first_bar_length)
            .into_iter()
            .map(|tempo| TuneLabTempo::new(tempo.position as f64, tempo.bpm))
            .collect()
    }

    fn generate_time_signatures(
        time_signature_list: &[TimeSignature],
    ) -> Vec<TuneLabTimeSignature> {
        time_signature_list
            .iter()
            .map(|time_signature| {
                TuneLabTimeSignature::new(
                    time_signature.numerator,
                    time_signature.denominator,
                    time_signature.bar_index,
                )
            })
            .collect()
    }

    fn generate_tracks(&mut self, track_list: &[Track]) -> Vec<TuneLabTrack> {
        let mut tlp_track_list = Vec::new();
        for track in track_list {
            match track {
                Track::Instrumental(track) => {
                    if let Some(track_info) = audio_track_info(&track.audio_file_path, false) {
                        let end_offset = self
                            .synchronizer
                            .get_actual_ticks_from_secs_offset(track.offset, track_info.duration);
                        tlp_track_list.push(TuneLabTrack::new(
                            track.title.clone(),
                            Self::generate_volume(track.volume),
                            track.pan,
                            track.mute,
                            track.solo,
                            vec![TuneLabPart::Audio(TuneLabAudioPart::new(
                                track.title.clone(),
                                track.offset as f64,
                                0.0,
                                end_offset,
                                track.audio_file_path.clone(),
                            ))],
                        ));
                    }
                }
                Track::Singing(track) if !track.note_list.is_empty() => {
                    let last_end = track.note_list.last().map(Note::end_pos).unwrap_or(0);
                    let end_offset = (last_end as f64 / self.first_bar_length as f64).ceil()
                        * self.first_bar_length as f64;
                    let mut part = TuneLabMidiPart::new(track.title.clone(), 0.0, 0.0, end_offset);
                    part.notes = Self::generate_notes(&track.note_list);
                    let pitch = self.generate_pitch(&track.edited_params.pitch);
                    if !pitch.is_empty() {
                        part.pitch = pitch;
                    }
                    tlp_track_list.push(TuneLabTrack::new(
                        track.title.clone(),
                        Self::generate_volume(track.volume),
                        track.pan,
                        track.mute,
                        track.solo,
                        vec![TuneLabPart::Midi(part)],
                    ));
                }
                _ => {}
            }
        }
        tlp_track_list
    }

    fn generate_volume(volume: f64) -> f64 {
        if volume > 0.0 {
            ratio_to_db(volume.max(0.01)).max(-70.0)
        } else {
            -70.0
        }
    }

    fn generate_notes(note_list: &[Note]) -> Vec<TuneLabNote> {
        note_list
            .iter()
            .map(|note| {
                let mut tlp_note = TuneLabNote::new(
                    note.start_pos as f64,
                    note.length as f64,
                    note.key_number,
                    note.lyric.clone(),
                );
                tlp_note.pronunciation = note.pronunciation.clone();
                tlp_note
            })
            .collect()
    }

    fn generate_pitch(&self, pitch: &ParamCurve) -> Vec<Vec<TuneLabPoint>> {
        let mut groups: Vec<Vec<TuneLabPoint>> = Vec::new();
        let mut buffer: Vec<TuneLabPoint> = Vec::new();
        for point in &pitch.points {
            if point.y == -100 {
                groups.push(std::mem::take(&mut buffer));
            } else {
                buffer.push(TuneLabPoint::new(
                    point.x as f64 - self.first_bar_length as f64,
                    point.y as f64 / 100.0,
                ));
            }
        }
        groups.push(buffer);
        groups.retain(|group| !group.is_empty());
        groups
    }
}
