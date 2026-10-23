//! NIAONiao project model (`nn`), port of `libresvip.plugins.nn.model` and
//! `libresvip.plugins.nn.template`.

use libresvip_core::options::format_float;

pub const DEFAULT_POINT_COUNT: usize = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct NNPoints {
    pub point_count: i64,
    pub points: Vec<i64>,
}

impl Default for NNPoints {
    fn default() -> Self {
        Self {
            point_count: DEFAULT_POINT_COUNT as i64,
            points: vec![50; DEFAULT_POINT_COUNT],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NNTimeSignature {
    pub numerator: i64,
    pub denominator: i64,
}

impl Default for NNTimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NNInfoLine {
    pub tempo: f64,
    pub time_signature: NNTimeSignature,
    pub bar_count: i64,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NNNote {
    pub lyric: String,
    pub pronunciation: String,
    pub start: i64,
    pub duration: i64,
    pub key: i64,
    pub cle: i64,
    pub vel: i64,
    pub por: i64,
    pub vibrato_length: i64,
    pub vibrato_depth: i64,
    pub vibrato_rate: i64,
    pub dynamics: NNPoints,
    pub pitch: NNPoints,
    pub pitch_bend_sensitivity: i64,
}

impl Default for NNNote {
    fn default() -> Self {
        Self {
            lyric: String::new(),
            pronunciation: String::new(),
            start: 0,
            duration: 0,
            key: 0,
            cle: 0,
            vel: 50,
            por: 0,
            vibrato_length: 0,
            vibrato_depth: 0,
            vibrato_rate: 0,
            dynamics: NNPoints::default(),
            pitch: NNPoints::default(),
            pitch_bend_sensitivity: 1,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NNProject {
    pub info_line: Option<NNInfoLine>,
    pub note_count: i64,
    pub notes: Vec<NNNote>,
}

/// Render an `NNProject` exactly like the Jinja template in Python.
pub fn render(project: &NNProject) -> String {
    let info_line = project.info_line.clone().unwrap_or(NNInfoLine {
        tempo: 120.0,
        time_signature: NNTimeSignature::default(),
        bar_count: 0,
        version: 19,
    });
    let mut output = String::new();
    output.push_str(&format!(
        "{} {} {} {} {} 0 0 0 0 0\n",
        format_float(info_line.tempo),
        info_line.time_signature.numerator,
        info_line.time_signature.denominator,
        info_line.bar_count,
        info_line.version,
    ));
    output.push_str(&format!("{}\n", project.note_count));
    for note in &project.notes {
        output.push_str(&format!(
            " {} {} {} {} {} {} {} {} {} {} {} {},{} {},{} {}\n",
            note.lyric,
            note.pronunciation,
            note.start,
            note.duration,
            note.key,
            note.cle,
            note.vel,
            note.por,
            note.vibrato_length,
            note.vibrato_depth,
            note.vibrato_rate,
            note.dynamics.point_count,
            join_points(&note.dynamics.points),
            note.pitch.point_count,
            join_points(&note.pitch.points),
            note.pitch_bend_sensitivity,
        ));
    }
    output
}

fn join_points(points: &[i64]) -> String {
    points
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
