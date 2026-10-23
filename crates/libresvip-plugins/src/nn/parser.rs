//! Port of `libresvip.plugins.nn.niaoniao_parser` (and the tatsu grammar in
//! `nn/model.py`, reimplemented as a line parser).

use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, Params, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::model::{NNInfoLine, NNNote, NNPoints, NNProject, NNTimeSignature};
use super::pinyin;

pub struct NiaoNiaoParser<'a> {
    options: &'a Options,
    length_multiplier: i64,
}

impl<'a> NiaoNiaoParser<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self {
            options,
            length_multiplier: 60,
        }
    }

    pub fn parse_project(&mut self, nn_project: &NNProject) -> Result<Project> {
        self.length_multiplier = if nn_project
            .info_line
            .as_ref()
            .map(|info| info.version == 19)
            .unwrap_or(false)
        {
            60
        } else {
            30
        };
        let info_line = nn_project
            .info_line
            .clone()
            .ok_or_else(|| LibreSvipError::Parse("missing NIAONiao info line".to_owned()))?;
        Ok(Project {
            version: String::new(),
            song_tempo_list: vec![SongTempo::new(0, info_line.tempo)],
            time_signature_list: vec![TimeSignature::new(
                0,
                info_line.time_signature.numerator,
                info_line.time_signature.denominator,
            )],
            track_list: vec![Track::Singing(SingingTrack {
                note_list: self.parse_notes(&nn_project.notes),
                edited_params: self.parse_params(&nn_project.notes)?,
                ..Default::default()
            })],
        })
    }

    fn parse_notes(&self, notes: &[NNNote]) -> Vec<Note> {
        notes
            .iter()
            .map(|nn_note| {
                let mut note = Note::new(
                    nn_note.start * self.length_multiplier,
                    nn_note.duration * self.length_multiplier,
                    83 - nn_note.key,
                    nn_note.lyric.clone(),
                );
                let phonemes = pinyin::heteronym(&nn_note.lyric);
                let keep_pronunciation = match phonemes.first() {
                    Some(readings) => {
                        readings.len() > 1
                            || readings
                                .first()
                                .map(|value| value != &nn_note.pronunciation)
                                .unwrap_or(false)
                    }
                    None => true,
                };
                if keep_pronunciation {
                    note.pronunciation = Some(nn_note.pronunciation.clone());
                }
                note
            })
            .collect()
    }

    fn parse_params(&self, notes: &[NNNote]) -> Result<Params> {
        let mut params = Params::default();
        for nn_note in notes {
            if self.options.bool("import_pitch", true)
                && nn_note.pitch.point_count > 0
                && nn_note.pitch.points.iter().any(|point| *point != 50)
            {
                if nn_note.pitch.point_count == 1 {
                    return Err(LibreSvipError::msg(
                        "pitch point count must be greater than 1",
                    ));
                }
                let step = (nn_note.duration * self.length_multiplier) as f64
                    / (nn_note.pitch.point_count - 1) as f64;
                let pbs = nn_note.pitch_bend_sensitivity + 1;
                params
                    .pitch
                    .points
                    .push(Point::new(nn_note.start * self.length_multiplier - 5 + 1920, -100));
                for i in 0..nn_note.pitch.point_count {
                    let point = *nn_note
                        .pitch
                        .points
                        .get(i as usize)
                        .ok_or_else(|| LibreSvipError::Parse("pitch point list is too short".to_owned()))?;
                    let x = py_round(
                        (nn_note.start * self.length_multiplier) as f64 + i as f64 * step,
                    ) + 1920;
                    let y = py_round(
                        ((point - 50) as f64 / 50.0 * pbs as f64
                            + (83 - nn_note.key) as f64)
                            * 100.0,
                    );
                    params.pitch.points.push(Point::new(x, y));
                }
                params.pitch.points.push(Point::new(
                    (nn_note.start + nn_note.duration) * self.length_multiplier + 5 + 1920,
                    -100,
                ));
            }
        }
        Ok(params)
    }
}

/// Python `round()` — half to even.
fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// Parse the textual NIAONiao project format.
pub fn parse_text(text: &str) -> Result<NNProject> {
    let mut lines = text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line));
    let info_line = lines
        .next()
        .ok_or_else(|| LibreSvipError::Parse("empty NIAONiao file".to_owned()))?;
    let info_line = parse_info_line(info_line)?;
    let note_count_line = lines
        .next()
        .ok_or_else(|| LibreSvipError::Parse("missing note count".to_owned()))?;
    let note_count = parse_int(note_count_line)?;

    let mut notes = Vec::new();
    for line in lines {
        if !line.starts_with(' ') {
            // `#CHORD` / `#DRUM` sections are not represented in the model.
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        notes.push(parse_note(line)?);
    }

    Ok(NNProject {
        info_line: Some(info_line),
        note_count,
        notes,
    })
}

fn parse_info_line(line: &str) -> Result<NNInfoLine> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 10 {
        return Err(LibreSvipError::Parse(
            "invalid NIAONiao info line".to_owned(),
        ));
    }
    Ok(NNInfoLine {
        tempo: parse_float(tokens[0])?,
        time_signature: NNTimeSignature {
            numerator: parse_int(tokens[1])?,
            denominator: parse_int(tokens[2])?,
        },
        bar_count: parse_int(tokens[3])?,
        version: parse_int(tokens[4])?,
    })
}

fn parse_note(line: &str) -> Result<NNNote> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 14 {
        return Err(LibreSvipError::Parse(
            "invalid NIAONiao note line".to_owned(),
        ));
    }
    Ok(NNNote {
        lyric: tokens[0].to_owned(),
        pronunciation: tokens[1].to_owned(),
        start: parse_int(tokens[2])?,
        duration: parse_int(tokens[3])?,
        key: parse_int(tokens[4])?,
        cle: parse_int(tokens[5])?,
        vel: parse_int(tokens[6])?,
        por: parse_int(tokens[7])?,
        vibrato_length: parse_int(tokens[8])?,
        vibrato_depth: parse_int(tokens[9])?,
        vibrato_rate: parse_int(tokens[10])?,
        dynamics: parse_points(tokens[11])?,
        pitch: parse_points(tokens[12])?,
        pitch_bend_sensitivity: parse_int(tokens[13])?,
    })
}

fn parse_points(token: &str) -> Result<NNPoints> {
    let mut parts = token.split(',');
    let count = parse_int(
        parts
            .next()
            .ok_or_else(|| LibreSvipError::Parse("missing point count".to_owned()))?,
    )?;
    let mut points = Vec::new();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        points.push(parse_int(part)?);
    }
    Ok(NNPoints {
        point_count: count,
        points,
    })
}

fn parse_int(token: &str) -> Result<i64> {
    token
        .trim()
        .parse::<i64>()
        .map_err(|_| LibreSvipError::Parse(format!("invalid integer: {token}")))
}

fn parse_float(token: &str) -> Result<f64> {
    token
        .trim()
        .parse::<f64>()
        .map_err(|_| LibreSvipError::Parse(format!("invalid float: {token}")))
}
