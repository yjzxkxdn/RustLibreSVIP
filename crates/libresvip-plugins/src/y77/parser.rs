//! Port of `libresvip.plugins.y77.y77_parser`.

use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, Params, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::model::Y77Project;
use super::pinyin;

pub struct Y77Parser<'a> {
    options: &'a Options,
    first_bar_length: i64,
}

impl<'a> Y77Parser<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self {
            options,
            first_bar_length: 0,
        }
    }

    pub fn parse_project(&mut self, y77_project: &Y77Project) -> Result<Project> {
        let time_signatures = self.parse_time_signatures(y77_project);
        let first = time_signatures
            .first()
            .ok_or_else(|| LibreSvipError::msg("missing time signature"))?;
        self.first_bar_length = round_ties_even(first.bar_length());
        Ok(Project {
            warnings: Vec::new(),
            version: String::new(),
            song_tempo_list: vec![SongTempo::new(0, y77_project.bpm)],
            time_signature_list: time_signatures,
            track_list: vec![Track::Singing(SingingTrack {
                ai_singer_name: "元七七".to_owned(),
                note_list: self.parse_notes(&y77_project.notes),
                edited_params: self.parse_params(&y77_project.notes)?,
                ..Default::default()
            })],
        })
    }

    fn parse_time_signatures(&self, y77_project: &Y77Project) -> Vec<TimeSignature> {
        vec![TimeSignature::new(
            0,
            y77_project.bbar,
            y77_project.bbeat,
        )]
    }

    fn parse_notes(&self, notes: &[super::model::Y77Note]) -> Vec<Note> {
        notes
            .iter()
            .map(|y77_note| {
                let mut note = Note::new(
                    y77_note.start * 30,
                    y77_note.length * 30,
                    88 - y77_note.pitch,
                    y77_note.lyric.clone(),
                );
                let phonemes = pinyin::heteronym(&y77_note.lyric);
                let keep_pronunciation = match phonemes.first() {
                    Some(readings) => {
                        readings.len() > 1
                            || readings
                                .first()
                                .map(|value| value != &y77_note.py)
                                .unwrap_or(false)
                    }
                    None => true,
                };
                if keep_pronunciation {
                    note.pronunciation = Some(y77_note.py.clone());
                }
                note
            })
            .collect()
    }

    fn parse_params(&self, notes: &[super::model::Y77Note]) -> Result<Params> {
        let mut params = Params::default();
        for y77_note in notes {
            if self.options.bool("import_pitch", true) && !y77_note.pit.is_empty() {
                if y77_note.pit.len() == 1 {
                    return Err(LibreSvipError::msg(
                        "pitch point count must be greater than 1",
                    ));
                }
                let step = (y77_note.length * 30) as f64 / (y77_note.pit.len() - 1) as f64;
                let pbs = y77_note.pbs + 1;
                params.pitch.points.push(Point::new(
                    y77_note.start * 30 - 5 + self.first_bar_length,
                    -100,
                ));
                for (i, pit) in y77_note.pit.iter().enumerate() {
                    let x = round_ties_even(
                        (y77_note.start * 30) as f64 + i as f64 * step,
                    ) + self.first_bar_length;
                    let y = round_ties_even(
                        ((pit.as_f64() - 50.0) / 50.0 * pbs as f64
                            + (88 - y77_note.pitch) as f64)
                            * 100.0,
                    );
                    params.pitch.points.push(Point::new(x, y));
                }
                params.pitch.points.push(Point::new(
                    (y77_note.start + y77_note.length) * 30 + 5 + self.first_bar_length,
                    -100,
                ));
            }
        }
        Ok(params)
    }
}

fn round_ties_even(value: f64) -> i64 {
    value.round_ties_even() as i64
}
