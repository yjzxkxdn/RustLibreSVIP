//! Port of `libresvip.plugins.ust.model` (the pydantic models only).

/// `MAX_ACCEPTED_BPM`.
pub const MAX_ACCEPTED_BPM: f64 = 10000.0;

/// `UTAUPitchBendMode` — `Literal["s", "r", "j", ""]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitchBendMode {
    S,
    R,
    J,
    Empty,
}

impl PitchBendMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PitchBendMode::S => "s",
            PitchBendMode::R => "r",
            PitchBendMode::J => "j",
            PitchBendMode::Empty => "",
        }
    }
}

/// `UTAUPitchBendType` — `Literal["OldData", "5"]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitchBendType {
    OldData,
    Five,
}

impl PitchBendType {
    pub fn as_str(self) -> &'static str {
        match self {
            PitchBendType::OldData => "OldData",
            PitchBendType::Five => "5",
        }
    }
}

/// One element of `PBS`/`PBW`/`PBY`: Python mixes ints, floats and the empty
/// string in these lists (`[1, *widths]`), and the rendered form differs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListValue {
    Int(i64),
    Float(f64),
    Empty,
}

impl ListValue {
    pub fn as_f64(self) -> Option<f64> {
        match self {
            ListValue::Int(value) => Some(value as f64),
            ListValue::Float(value) => Some(value),
            ListValue::Empty => None,
        }
    }
}

/// `UtauNoteVibrato`.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteVibrato {
    /// Percentage of the note's length.
    pub length: f64,
    /// MilliSec.
    pub period: f64,
    /// Cent.
    pub depth: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub phase_shift: f64,
    pub shift: f64,
}

impl NoteVibrato {
    /// Only `length` and `period` are required by the format; the remaining
    /// fields default to zero.
    #[allow(dead_code)]
    pub fn new(length: f64, period: f64) -> Self {
        Self {
            length,
            period,
            depth: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            phase_shift: 0.0,
            shift: 0.0,
        }
    }
}

/// `UTAUEnvelope`. `other_points` distinguishes "absent" (`None`) from "the
/// `{"," float}` alternative matched zero times" (`Some(vec![])`).
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    pub p1: f64,
    pub p2: f64,
    pub p3: f64,
    pub v1: f64,
    pub v2: f64,
    pub v3: f64,
    pub v4: f64,
    pub p4: Option<f64>,
    pub p5: Option<f64>,
    pub v5: Option<f64>,
    pub other_points: Option<Vec<f64>>,
}

/// `UTAUNote`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Note {
    pub note_type: String,
    pub length: i64,
    pub lyric: String,
    pub note_num: i64,
    pub duration: Option<i64>,
    pub delta: Option<i64>,
    pub pre_utterance: Option<String>,
    pub label: Option<String>,
    pub flags: Option<String>,
    pub envelope: Option<Envelope>,
    pub voice_overlap: Option<f64>,
    pub intensity: Option<f64>,
    pub modulation: Option<f64>,
    pub start_point: Option<f64>,
    pub tempo: Option<f64>,
    pub velocity: Option<f64>,
    pub pitchbend_start: Option<f64>,
    pub pitchbend_type: Option<PitchBendType>,
    pub pitch_bend_points: Vec<i64>,
    pub pbs: Vec<ListValue>,
    pub pbw: Vec<ListValue>,
    pub pby: Vec<ListValue>,
    pub pbm: Vec<PitchBendMode>,
    pub vbr: Option<NoteVibrato>,
    pub stp_trim: Option<f64>,
    pub layer: Option<i64>,
    pub at_preutterance: Option<f64>,
    pub at_overlap: Option<f64>,
    pub at_start_point: Option<f64>,
    pub sample_filename: Option<String>,
    pub alias: Option<String>,
    pub cache_location: Option<String>,
}

/// `UTAUTrack`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub notes: Vec<Note>,
}

/// `UTAUTimeSignature`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimeSignature {
    pub numerator: i64,
    pub denominator: i64,
    pub bar_index: i64,
}

/// `UTAUProject`.
#[derive(Clone, Debug, PartialEq)]
pub struct UtauProject {
    pub ust_version: Option<f64>,
    pub project_name: String,
    pub track_count: i64,
    pub pitch_mode2: bool,
    pub charset: Option<String>,
    pub tempo: Option<f64>,
    pub voice_dir: Option<String>,
    pub out_file: Option<String>,
    pub cache_dir: Option<String>,
    pub tool1: Option<String>,
    pub tool2: Option<String>,
    pub flags: Option<String>,
    pub autoren: Option<bool>,
    pub map_first: Option<bool>,
    pub time_signatures: Vec<TimeSignature>,
    pub track: Vec<Track>,
}

impl Default for UtauProject {
    fn default() -> Self {
        Self {
            ust_version: Some(1.2),
            project_name: "New Project".to_owned(),
            track_count: 1,
            pitch_mode2: false,
            charset: None,
            tempo: None,
            voice_dir: None,
            out_file: None,
            cache_dir: None,
            tool1: None,
            tool2: None,
            flags: None,
            autoren: None,
            map_first: None,
            time_signatures: Vec::new(),
            track: Vec::new(),
        }
    }
}
