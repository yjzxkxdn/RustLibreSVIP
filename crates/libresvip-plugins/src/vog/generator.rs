//! Port of `libresvip.plugins.vog.vogen_generator`.

use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::Options;

use super::model::{VogenNote, VogenProject, VogenTrack};
use super::pinyin;

pub struct VogenGenerator<'a> {
    #[allow(dead_code)]
    options: &'a Options,
}

impl<'a> VogenGenerator<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self { options }
    }

    pub fn generate_project(&self, project: &Project) -> Result<VogenProject> {
        let tempo = project
            .song_tempo_list
            .first()
            .ok_or_else(|| LibreSvipError::msg("no tempo found"))?;
        let time_signature = project
            .time_signature_list
            .first()
            .ok_or_else(|| LibreSvipError::msg("missing time signature"))?;
        Ok(VogenProject {
            time_sig0: format!(
                "{}/{}",
                time_signature.numerator, time_signature.denominator
            ),
            bpm0: tempo.bpm,
            accom_offset: 0,
            utts: project
                .track_list
                .iter()
                .filter_map(|track| track.as_singing())
                .map(|track| VogenTrack {
                    name: Some(track.title.clone()),
                    singer_id: Some(track.ai_singer_name.clone()),
                    rom_scheme: String::new(),
                    notes: track
                        .note_list
                        .iter()
                        .map(|note| VogenNote {
                            pitch: Some(note.key_number),
                            lyric: Some(note.lyric.clone()),
                            rom: Some(match &note.pronunciation {
                                Some(value) if !value.is_empty() => value.clone(),
                                _ => pinyin::lazy_pinyin(&note.lyric).join(" "),
                            }),
                            on: Some(note.start_pos),
                            dur: Some(note.length),
                        })
                        .collect(),
                })
                .collect(),
        })
    }
}
