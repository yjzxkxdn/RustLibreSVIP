//! Port of `libresvip.plugins.vog.vogen_parser`.

use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::options::Options;

use super::model::{VogenNote, VogenProject};

pub struct VogenParser<'a> {
    #[allow(dead_code)]
    options: &'a Options,
}

impl<'a> VogenParser<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self { options }
    }

    pub fn parse_project(&self, vogen_project: &VogenProject) -> Result<Project> {
        Ok(Project {
            version: String::new(),
            song_tempo_list: vec![SongTempo::new(0, vogen_project.bpm0)],
            time_signature_list: vec![self.parse_time_signature(&vogen_project.time_sig0)?],
            track_list: vogen_project
                .utts
                .iter()
                .map(|utt| {
                    Track::Singing(SingingTrack {
                        title: utt.name.clone().unwrap_or_default(),
                        ai_singer_name: utt.singer_id.clone().unwrap_or_default(),
                        note_list: self.parse_notes(&utt.notes),
                        ..Default::default()
                    })
                })
                .collect(),
        })
    }

    fn parse_time_signature(&self, time_sig0: &str) -> Result<TimeSignature> {
        let (numerator, denominator) = time_sig0.split_once('/').ok_or_else(|| {
            LibreSvipError::Validation(format!("invalid time signature: {time_sig0}"))
        })?;
        let numerator = numerator
            .trim()
            .parse::<i64>()
            .map_err(|_| LibreSvipError::Validation(format!("invalid time signature: {time_sig0}")))?;
        let denominator = denominator
            .trim()
            .parse::<i64>()
            .map_err(|_| LibreSvipError::Validation(format!("invalid time signature: {time_sig0}")))?;
        Ok(TimeSignature::new(0, numerator, denominator))
    }

    fn parse_notes(&self, notes: &[VogenNote]) -> Vec<Note> {
        notes
            .iter()
            .map(|note| Note {
                start_pos: note.on.unwrap_or(0),
                length: note.dur.unwrap_or(0),
                key_number: note.pitch.unwrap_or(0),
                lyric: note.lyric.clone().unwrap_or_default(),
                pronunciation: note.rom.clone(),
                ..Default::default()
            })
            .collect()
    }
}
