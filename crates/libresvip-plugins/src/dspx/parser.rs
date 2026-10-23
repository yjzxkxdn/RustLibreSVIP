//! Port of `libresvip.plugins.dspx.dspx_parser`.

use std::path::Path;

use libresvip_core::core::constants::DEFAULT_BPM;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::options::Options;

use super::model::{AudioClip, BusControl, Clip, DspxModel, SingingClip, Track as DspxTrack};
use super::param_utils::{build_pitch_curve, PitchImportMode, ResolvedParam};
use super::pyutil::{clamp, db_to_float, py_round, resolve_path};
use super::vibrato_utils::{import_vibrato, VibratoSequence};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VibratoImportMode {
    None,
    Preserve,
    BakeToPitch,
}

pub struct DspxParser {
    import_pitch: bool,
    pitch_import_mode: PitchImportMode,
    vibrato_import_mode: VibratoImportMode,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
    master_control: BusControl,
    project_path: std::path::PathBuf,
}

impl DspxParser {
    pub fn new(options: &Options, path: &Path) -> Self {
        let pitch_import_mode = match options
            .str("pitch_import_mode", "edited-and-original")
            .as_str()
        {
            "edited-only" => PitchImportMode::EditedOnly,
            _ => PitchImportMode::EditedAndOriginal,
        };
        let vibrato_import_mode = match options.str("vibrato_import_mode", "preserve").as_str() {
            "none" => VibratoImportMode::None,
            "bake-to-pitch" => VibratoImportMode::BakeToPitch,
            _ => VibratoImportMode::Preserve,
        };
        Self {
            import_pitch: options.bool("import_pitch", true),
            pitch_import_mode,
            vibrato_import_mode,
            first_bar_length: 0,
            synchronizer: TimeSynchronizer::from_project(&[]),
            master_control: BusControl::default(),
            project_path: path.to_path_buf(),
        }
    }

    pub fn parse_project(&mut self, model: &DspxModel) -> Project {
        let tempos = Self::parse_tempos(model);
        let time_signatures = Self::parse_time_signatures(model);
        self.first_bar_length = time_signatures
            .first()
            .map(|signature| py_round(signature.bar_length()))
            .unwrap_or(0);
        self.synchronizer = TimeSynchronizer::from_project(&tempos);
        self.master_control = model.content.master.control.clone();
        Project {
            version: model.version.clone(),
            song_tempo_list: tempos,
            time_signature_list: time_signatures,
            track_list: self.parse_tracks(&model.content.tracks),
        }
    }

    fn parse_tempos(model: &DspxModel) -> Vec<SongTempo> {
        let mut by_position: std::collections::BTreeMap<i64, &super::model::Tempo> =
            std::collections::BTreeMap::new();
        for tempo in &model.content.timeline.tempos {
            by_position.insert(tempo.pos, tempo);
        }
        let mut tempos: Vec<SongTempo> = by_position
            .into_values()
            .map(|tempo| SongTempo {
                position: tempo.pos,
                bpm: tempo.value,
            })
            .collect();
        if tempos.is_empty() || tempos[0].position > 0 {
            tempos.insert(
                0,
                SongTempo {
                    position: 0,
                    bpm: DEFAULT_BPM,
                },
            );
        }
        tempos
    }

    fn parse_time_signatures(model: &DspxModel) -> Vec<TimeSignature> {
        let mut by_index: std::collections::BTreeMap<i64, &super::model::TimeSignature> =
            std::collections::BTreeMap::new();
        for signature in &model.content.timeline.time_signatures {
            by_index.insert(signature.index, signature);
        }
        let mut time_signatures: Vec<TimeSignature> = by_index
            .into_values()
            .map(|signature| TimeSignature {
                bar_index: signature.index,
                numerator: signature.numerator,
                denominator: signature.denominator,
            })
            .collect();
        if time_signatures.is_empty() || time_signatures[0].bar_index > 0 {
            time_signatures.insert(
                0,
                TimeSignature {
                    bar_index: 0,
                    numerator: 4,
                    denominator: 4,
                },
            );
        }
        time_signatures
    }

    fn parse_tracks(&mut self, tracks: &[DspxTrack]) -> Vec<Track> {
        let mut result = Vec::new();
        for track in tracks {
            for clip in &track.clips {
                match clip {
                    Clip::Audio(audio) => {
                        result.push(Track::Instrumental(self.parse_audio_clip(track, audio)))
                    }
                    Clip::Singing(singing) => {
                        result.push(Track::Singing(self.parse_singing_clip(track, singing)))
                    }
                }
            }
        }
        result
    }

    fn parse_common_track_fields(
        &self,
        track: &DspxTrack,
        name: &str,
        control: &BusControl,
    ) -> CommonTrackFields {
        let total_gain = clamp(
            self.master_control.gain + track.control.control.gain + control.gain,
            -400.0,
            400.0,
        );
        CommonTrackFields {
            title: if name.is_empty() {
                track.name.clone()
            } else {
                name.to_owned()
            },
            volume: db_to_float(total_gain),
            pan: clamp(
                self.master_control.pan + track.control.control.pan + control.pan,
                -1.0,
                1.0,
            ),
            mute: self.master_control.mute || track.control.control.mute || control.mute,
            solo: track.control.solo,
        }
    }

    fn parse_audio_clip(&self, track: &DspxTrack, clip: &AudioClip) -> InstrumentalTrack {
        let common = self.parse_common_track_fields(track, &clip.name, &clip.control);
        let audio_path = resolve_path(
            &clip.path,
            self.project_path.parent().unwrap_or_else(|| Path::new("")),
        );
        InstrumentalTrack {
            title: common.title,
            mute: common.mute,
            solo: common.solo,
            volume: common.volume,
            pan: common.pan,
            audio_file_path: audio_path,
            offset: clip.time.pos - clip.time.clip_start,
            ..Default::default()
        }
    }

    fn parse_singing_clip(&self, track: &DspxTrack, clip: &SingingClip) -> SingingTrack {
        let common = self.parse_common_track_fields(track, &clip.name, &clip.control);
        let clip_start = clip.time.pos - clip.time.clip_start;
        let source_notes: Vec<&super::model::Note> = clip
            .notes
            .iter()
            .filter(|note| note.length > 0 && !note.lyric.trim().is_empty())
            .collect();
        let note_list: Vec<Note> = source_notes
            .iter()
            .map(|note| self.parse_note(note, clip_start))
            .collect();
        let mut singing_track = SingingTrack {
            title: common.title,
            mute: common.mute,
            solo: common.solo,
            volume: common.volume,
            pan: common.pan,
            note_list,
            ..Default::default()
        };
        if self.import_pitch {
            if let Some(pitch_parameter) = clip.params.get("pitch") {
                let pitch = ResolvedParam::new(pitch_parameter, self.pitch_import_mode);
                let coordinate_offset = clip_start + self.first_bar_length;
                if self.vibrato_import_mode == VibratoImportMode::BakeToPitch {
                    let sequence = std::cell::RefCell::new(VibratoSequence::new(
                        &clip.notes,
                        clip_start,
                        &self.synchronizer,
                    ));
                    let vibrato = |relative_tick: i64| {
                        sequence.borrow_mut().evaluate(relative_tick, clip_start)
                    };
                    singing_track.edited_params.pitch =
                        build_pitch_curve(&pitch, coordinate_offset, Some(&vibrato));
                } else {
                    singing_track.edited_params.pitch =
                        build_pitch_curve(&pitch, coordinate_offset, None);
                }
            }
        }
        singing_track
    }

    fn parse_note(&self, note: &super::model::Note, clip_start: i64) -> Note {
        let vibrato = if self.vibrato_import_mode == VibratoImportMode::Preserve {
            Some(import_vibrato(&note.vibrato))
        } else {
            None
        };
        Note {
            start_pos: note.pos + clip_start,
            length: note.length,
            key_number: note.key_num,
            lyric: note.lyric.clone(),
            vibrato,
            ..Default::default()
        }
    }
}

struct CommonTrackFields {
    title: String,
    volume: f64,
    pan: f64,
    mute: bool,
    solo: bool,
}
