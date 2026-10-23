//! Port of `libresvip.plugins.vspx.model`.
//!
//! The VSPX format is an XML document (`<VSPX><Version/><Project/>…`). This
//! module holds the in-memory representation; `xml` renders it exactly like the
//! xsdata serializer configured by `vocalsharp_converter` and parses it back.

use libresvip_core::model::point::Point;

/// A Python number for fields that keep their int/float type across a dump.
///
/// pydantic does not validate defaults, so `VocalSharpTrillBase(amplitude=0)`
/// keeps an `int` that `str()` renders as `0`, while a parsed value is coerced
/// to `float` and renders as `0.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    pub fn as_f64(self) -> f64 {
        match self {
            Num::Int(value) => value as f64,
            Num::Float(value) => value,
        }
    }
}

impl Default for Num {
    fn default() -> Self {
        Num::Int(0)
    }
}

impl From<i64> for Num {
    fn from(value: i64) -> Self {
        Num::Int(value)
    }
}

impl From<f64> for Num {
    fn from(value: f64) -> Self {
        Num::Float(value)
    }
}

/// `VocalSharpTrill` / `VocalSharpDefaultTrill`.
#[derive(Clone, Copy, Debug)]
pub struct Trill {
    pub pos: Num,
    pub amplitude: Num,
    pub frequency: Num,
    pub phase: Num,
}

impl Default for Trill {
    fn default() -> Self {
        Self {
            pos: Num::Float(0.25),
            amplitude: Num::Int(0),
            frequency: Num::Float(5.5),
            phase: Num::Int(0),
        }
    }
}

/// `VocalSharpParamBase` subclasses: the `B`/`D`/… parameter points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Bre,
    Dyn,
    Gen,
    Pit,
    Bri,
    Str,
    Voc,
    Gwl,
    Xsy,
}

impl ParamKind {
    pub fn tag(self) -> &'static str {
        match self {
            ParamKind::Bre => "B",
            ParamKind::Dyn => "D",
            ParamKind::Gen => "G",
            ParamKind::Pit => "P",
            ParamKind::Bri => "R",
            ParamKind::Str => "S",
            ParamKind::Voc => "V",
            ParamKind::Gwl => "W",
            ParamKind::Xsy => "X",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "B" => Some(ParamKind::Bre),
            "D" => Some(ParamKind::Dyn),
            "G" => Some(ParamKind::Gen),
            "P" => Some(ParamKind::Pit),
            "R" => Some(ParamKind::Bri),
            "S" => Some(ParamKind::Str),
            "V" => Some(ParamKind::Voc),
            "W" => Some(ParamKind::Gwl),
            "X" => Some(ParamKind::Xsy),
            _ => None,
        }
    }
}

/// One `<P><t>…</t><v>…</v></P>` style parameter point.
#[derive(Clone, Copy, Debug)]
pub struct ParamPoint {
    pub kind: ParamKind,
    pub time: i64,
    pub value: i64,
}

impl ParamPoint {
    pub fn pit(time: i64, value: i64) -> Self {
        Self {
            kind: ParamKind::Pit,
            time,
            value,
        }
    }
}

/// `VocalSharpBeat`.
#[derive(Clone, Copy, Debug)]
pub struct Beat {
    pub bar_index: i64,
    pub beat_per_bar: i64,
    pub bar_divide: i64,
}

impl Default for Beat {
    fn default() -> Self {
        Self {
            bar_index: 0,
            beat_per_bar: 4,
            bar_divide: 4,
        }
    }
}

/// `VocalSharpTempo`.
#[derive(Clone, Copy, Debug)]
pub struct Tempo {
    pub pos: i64,
    pub bpm: f64,
}

impl Default for Tempo {
    fn default() -> Self {
        Self { pos: 0, bpm: 120.0 }
    }
}

/// `VocalSharpDefaultParameter`.
#[derive(Clone, Copy, Debug)]
pub struct DefaultParameter {
    pub dyn_value: i64,
    pub strength: i64,
    pub bri: i64,
    pub voc: i64,
    pub bre: i64,
    pub gen: i64,
    pub gwl: i64,
    pub xsy: i64,
}

impl Default for DefaultParameter {
    fn default() -> Self {
        Self {
            dyn_value: 128,
            strength: 128,
            bri: 128,
            voc: 255,
            bre: 128,
            gen: 128,
            gwl: 0,
            xsy: 0,
        }
    }
}

/// `VocalSharpSequence`.
#[derive(Clone, Debug, Default)]
pub struct Sequence {
    pub pos: Option<i64>,
    pub name: Option<String>,
    pub path: Option<String>,
}

/// `VocalSharpSyllablePartBase`.
#[derive(Clone, Debug, Default)]
pub struct SyllablePart {
    pub points: Vec<Point>,
    pub sample: Option<String>,
    pub symbol: Option<String>,
}

/// `VocalSharpSyllable`.
#[derive(Clone, Debug, Default)]
pub struct Syllable {
    pub head: Option<SyllablePart>,
    pub cur: Option<SyllablePart>,
    pub tail: Option<SyllablePart>,
}

/// `VocalSharpNote`.
#[derive(Clone, Debug)]
pub struct Note {
    pub pitch: String,
    pub pos: i64,
    pub duration: i64,
    pub lyric: Option<String>,
    pub trill: Option<Trill>,
    pub syllables: Vec<Syllable>,
}

impl Note {
    /// `VocalSharpNote.key_number`.
    pub fn key_number(&self) -> Result<i64, String> {
        super::music_math::note2midi(&self.pitch)
    }
}

/// Common fields of `VocalSharpTrackBase`.
#[derive(Clone, Debug)]
pub struct TrackBase {
    pub name: Option<String>,
    pub pan: Num,
    pub gain: Num,
    pub is_mute: Option<String>,
    pub is_solo: Option<String>,
}

impl Default for TrackBase {
    fn default() -> Self {
        Self {
            name: Some("神秘轨道_0".to_owned()),
            pan: Num::Int(0),
            gain: Num::Int(0),
            is_mute: Some("False".to_owned()),
            is_solo: Some("False".to_owned()),
        }
    }
}

/// `VocalSharpNoteTrack`.
#[derive(Clone, Debug)]
pub struct NoteTrack {
    pub name: Option<String>,
    pub pan: Num,
    pub gain: Num,
    pub is_mute: Option<String>,
    pub is_solo: Option<String>,
    pub singer: Option<String>,
    pub lsd: Option<String>,
    pub default_parameter: DefaultParameter,
    pub por: Num,
    pub notes: Vec<Note>,
    pub parameter: Vec<ParamPoint>,
}

impl Default for NoteTrack {
    fn default() -> Self {
        Self {
            name: Some("神秘轨道_0".to_owned()),
            pan: Num::Int(0),
            gain: Num::Int(0),
            is_mute: Some("False".to_owned()),
            is_solo: Some("False".to_owned()),
            singer: Some("神秘歌手".to_owned()),
            lsd: Some("神秘字典".to_owned()),
            default_parameter: DefaultParameter::default(),
            por: Num::Float(0.07),
            notes: Vec::new(),
            parameter: Vec::new(),
        }
    }
}

impl NoteTrack {
    pub fn is_muted(&self) -> bool {
        self.is_mute.as_deref() == Some("True")
    }

    pub fn is_soloed(&self) -> bool {
        self.is_solo.as_deref() == Some("True")
    }
}

/// `VocalSharpStereoTrack` / `VocalSharpMonoTrack`.
#[derive(Clone, Debug)]
pub struct InstrumentalTrack {
    pub name: Option<String>,
    pub pan: Num,
    pub gain: Num,
    pub is_mute: Option<String>,
    pub is_solo: Option<String>,
    pub sequences: Vec<Sequence>,
}

impl Default for InstrumentalTrack {
    fn default() -> Self {
        Self {
            name: Some("神秘轨道_0".to_owned()),
            pan: Num::Int(0),
            gain: Num::Int(0),
            is_mute: Some("False".to_owned()),
            is_solo: Some("False".to_owned()),
            sequences: Vec::new(),
        }
    }
}

impl InstrumentalTrack {
    pub fn is_muted(&self) -> bool {
        self.is_mute.as_deref() == Some("True")
    }

    pub fn is_soloed(&self) -> bool {
        self.is_solo.as_deref() == Some("True")
    }
}

/// The `NoteTrack`/`StereoTrack`/`MonoTrack` union (`Project/tracks`).
#[derive(Clone, Debug)]
pub enum Track {
    Note(NoteTrack),
    Stereo(InstrumentalTrack),
    Mono(InstrumentalTrack),
}

/// `VocalSharpInnerProject`.
#[derive(Clone, Debug)]
pub struct InnerProject {
    pub samples_per_sec: i64,
    pub resolution: i64,
    pub duration: Option<i64>,
    pub default_trill: Option<Trill>,
    pub tempos: Vec<Tempo>,
    pub beats: Vec<Beat>,
    pub tracks: Vec<Track>,
}

impl Default for InnerProject {
    fn default() -> Self {
        Self {
            samples_per_sec: 44100,
            resolution: 1920,
            duration: None,
            default_trill: Some(Trill::default()),
            tempos: Vec::new(),
            beats: Vec::new(),
            tracks: Vec::new(),
        }
    }
}

/// `VocalSharpProject`.
#[derive(Clone, Debug)]
pub struct VspxProject {
    pub version: Option<String>,
    pub project: InnerProject,
}

impl Default for VspxProject {
    fn default() -> Self {
        Self {
            version: Some("Beta0.0.0".to_owned()),
            project: InnerProject::default(),
        }
    }
}
