//! Pydantic-compatible model of the DSPX 1.0.0 exchange format.
//!
//! Field order mirrors `libresvip.plugins.dspx.model_v1`; `serde_json` writes
//! struct fields in declaration order, which is what Python's `model_dump`
//! produces as well.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub const INT32_MAX: i64 = 2_147_483_647;
pub const INT32_MIN: i64 = -2_147_483_648;

pub type Workspace = serde_json::Map<String, serde_json::Value>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DspxModel {
    pub content: Content,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Content {
    #[serde(rename = "global")]
    pub global: Global,
    pub master: Master,
    pub timeline: Timeline,
    pub tracks: Vec<Track>,
    pub workspace: Workspace,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Global {
    pub author: String,
    #[serde(rename = "centShift", alias = "cent_shift")]
    pub cent_shift: i64,
    #[serde(rename = "editorId", alias = "editor_id")]
    pub editor_id: String,
    #[serde(rename = "editorName", alias = "editor_name")]
    pub editor_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Master {
    pub control: BusControl,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BusControl {
    pub gain: f64,
    pub mute: bool,
    pub pan: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackControl {
    #[serde(flatten)]
    pub control: BusControl,
    pub solo: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Timeline {
    pub labels: Vec<Label>,
    pub tempos: Vec<Tempo>,
    #[serde(rename = "timeSignatures", alias = "time_signatures")]
    pub time_signatures: Vec<TimeSignature>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Label {
    pub pos: i64,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Tempo {
    pub pos: i64,
    pub value: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeSignature {
    pub index: i64,
    pub numerator: i64,
    pub denominator: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub name: String,
    pub control: TrackControl,
    pub clips: Vec<Clip>,
    pub workspace: Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Clip {
    #[serde(rename = "audio")]
    Audio(AudioClip),
    #[serde(rename = "singing")]
    Singing(SingingClip),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioClip {
    pub name: String,
    pub path: String,
    pub time: ClipTime,
    pub control: BusControl,
    pub workspace: Workspace,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SingingClip {
    pub name: String,
    pub time: ClipTime,
    pub control: BusControl,
    pub notes: Vec<Note>,
    pub params: IndexMap<String, Param>,
    pub sources: Option<serde_json::Value>,
    pub workspace: Workspace,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipTime {
    #[serde(rename = "clipLen", alias = "clip_len")]
    pub clip_len: i64,
    #[serde(rename = "clipStart", alias = "clip_start")]
    pub clip_start: i64,
    pub length: i64,
    pub pos: i64,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VibratoPoints {
    pub amp: Vec<ControlPoint>,
    pub freq: Vec<ControlPoint>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Vibrato {
    pub start: f64,
    pub end: f64,
    pub amp: i64,
    pub freq: f64,
    pub phase: f64,
    pub offset: i64,
    pub points: VibratoPoints,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pronunciation {
    pub original: String,
    pub edited: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Phoneme {
    pub language: String,
    pub start: i64,
    pub token: String,
    pub onset: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Phonemes {
    pub original: Vec<Phoneme>,
    pub edited: Vec<Phoneme>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    pub pos: i64,
    pub length: i64,
    #[serde(rename = "keyNum", alias = "key_num")]
    pub key_num: i64,
    #[serde(rename = "centShift", alias = "cent_shift")]
    pub cent_shift: i64,
    pub language: String,
    pub lyric: String,
    pub pronunciation: Pronunciation,
    pub phonemes: Phonemes,
    pub vibrato: Vibrato,
    pub workspace: Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParamCurve {
    #[serde(rename = "free")]
    Free {
        start: i64,
        step: i64,
        values: Vec<i64>,
    },
    #[serde(rename = "anchor")]
    Anchor {
        start: i64,
        nodes: Vec<AnchorNode>,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AnchorNode {
    pub interp: String,
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Param {
    pub edited: Vec<ParamCurve>,
    pub transform: Vec<ParamCurve>,
    pub original: Vec<ParamCurve>,
}
