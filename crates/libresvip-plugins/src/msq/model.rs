//! Port of `libresvip.experimental.msq.model` (Mikoto Studio sequence).

use serde::{Deserialize, Serialize};

use libresvip_core::utils::text::uuid_str;

fn default_uuid() -> String {
    uuid_str()
}

fn default_main_out_name() -> String {
    "Main Out".to_owned()
}

fn default_sends() -> Vec<serde_json::Value> {
    Vec::new()
}

fn default_expressions() -> Vec<String> {
    Vec::new()
}

fn default_language() -> String {
    "SingerDefault".to_owned()
}

fn default_parameter() -> MsqParameter {
    MsqParameter::default()
}

fn default_settings_legacy() -> bool {
    false
}

fn default_settings() -> MsqSettings {
    MsqSettings::default()
}

fn default_version() -> i64 {
    110
}

fn default_aux() -> Vec<serde_json::Value> {
    Vec::new()
}

fn default_tracks() -> Vec<MsqTrack> {
    Vec::new()
}

fn default_effects() -> serde_json::Value {
    serde_json::Value::Null
}

fn default_track_children() -> Vec<serde_json::Value> {
    Vec::new()
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MsqTimeSignature {
    pub numerator: i64,
    pub denominator: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MsqBpmEvent {
    pub position: i64,
    pub bpm: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MsqTimeSignatureEvent {
    pub position: i64,
    pub time_sig: MsqTimeSignature,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqSetup {
    pub ppq: f64,
    pub bpm: f64,
    pub time_signature: MsqTimeSignature,
    pub bpm_events: Vec<MsqBpmEvent>,
    pub time_signature_events: Vec<MsqTimeSignatureEvent>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MsqSettings {
    #[serde(default = "default_settings_legacy")]
    pub legacy_input: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqMainOut {
    #[serde(default = "default_uuid")]
    pub id: String,
    #[serde(default = "default_main_out_name")]
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqMixer {
    pub gain: f64,
    pub pan: f64,
    pub record_arm: bool,
    pub solo: bool,
    pub mute: bool,
    pub output: String,
    #[serde(default = "default_uuid")]
    pub bus: String,
    #[serde(default = "default_sends")]
    pub sends: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqSinger {
    pub name: String,
    pub id: String,
    #[serde(default = "default_uuid")]
    pub primary_expression: String,
    #[serde(default = "default_expressions")]
    pub loaded_expressions: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_language")]
    pub accent: String,
    #[serde(default = "default_language")]
    pub phoneset: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqSingerInstrument {
    #[serde(rename = "Singer")]
    pub singer: MsqSinger,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqColor {
    #[serde(rename = "TrackColor")]
    pub track_color: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqPhoneme {
    pub phoneme: String,
    pub time: i64,
    pub velocity: i64,
    pub lock_timing: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqNote {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub clip_id: String,
    pub start: i64,
    pub length: i64,
    pub pitch: f64,
    pub lyric: String,
    pub phonemes: Vec<MsqPhoneme>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MsqPoint {
    pub time: i64,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqParameter {
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub points: Vec<MsqPoint>,
}

impl Default for MsqParameter {
    fn default() -> Self {
        Self {
            value: 0.0,
            points: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqParameters {
    #[serde(rename = "Breathiness", default = "default_parameter")]
    pub breathiness: MsqParameter,
    #[serde(rename = "Tension", default = "default_parameter")]
    pub tension: MsqParameter,
    #[serde(rename = "PitchTransition", default = "default_parameter")]
    pub pitch_transition: MsqParameter,
    #[serde(rename = "VibratoRate", default = "default_parameter")]
    pub vibrato_rate: MsqParameter,
    #[serde(rename = "Character", default = "default_parameter")]
    pub character: MsqParameter,
    #[serde(rename = "VibratoDepth", default = "default_parameter")]
    pub vibrato_depth: MsqParameter,
    #[serde(rename = "Pitch", default = "default_parameter")]
    pub pitch: MsqParameter,
    #[serde(rename = "Roughness", default = "default_parameter")]
    pub roughness: MsqParameter,
    #[serde(rename = "Stability", default = "default_parameter")]
    pub stability: MsqParameter,
    #[serde(rename = "Dynamics", default = "default_parameter")]
    pub dynamics: MsqParameter,
}

impl Default for MsqParameters {
    fn default() -> Self {
        Self {
            breathiness: MsqParameter::default(),
            tension: MsqParameter::default(),
            pitch_transition: MsqParameter::default(),
            vibrato_rate: MsqParameter::default(),
            character: MsqParameter::default(),
            vibrato_depth: MsqParameter::default(),
            pitch: MsqParameter::default(),
            roughness: MsqParameter::default(),
            stability: MsqParameter::default(),
            dynamics: MsqParameter::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqAudio {
    pub path: String,
    pub start: f64,
    pub speed: f64,
    pub bpm: f64,
}

/// `MsqBaseClip` fields shared by both clip kinds (the discriminated unions
/// below flatten them, like pydantic's tagged unions).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqSingerClip {
    #[serde(default = "default_uuid")]
    pub id: String,
    #[serde(default)]
    pub clip_id: Option<String>,
    pub track_id: String,
    pub name: String,
    pub color: serde_json::Value,
    pub start: i64,
    pub length: i64,
    #[serde(rename = "kind")]
    pub kind: String,
    #[serde(default)]
    pub notes: Option<Vec<MsqNote>>,
    #[serde(default)]
    pub parameters: Option<MsqParameters>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqAudioClip {
    #[serde(default = "default_uuid")]
    pub id: String,
    #[serde(default)]
    pub clip_id: Option<String>,
    pub track_id: String,
    pub name: String,
    pub color: serde_json::Value,
    pub start: i64,
    pub length: i64,
    #[serde(rename = "kind")]
    pub kind: String,
    #[serde(default)]
    pub audio: Option<MsqAudio>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqSingerTrack {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub mixer: MsqMixer,
    pub name: String,
    pub color: String,
    pub index: i64,
    #[serde(default = "default_effects")]
    pub effects: serde_json::Value,
    #[serde(default = "default_track_children")]
    pub tracks: Vec<serde_json::Value>,
    #[serde(rename = "track_type")]
    pub track_type: String,
    pub instrument: MsqSingerInstrument,
    pub clips: Vec<MsqSingerClip>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MsqAudioTrack {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub mixer: MsqMixer,
    pub name: String,
    pub color: String,
    pub index: i64,
    #[serde(default = "default_effects")]
    pub effects: serde_json::Value,
    #[serde(default = "default_track_children")]
    pub tracks: Vec<serde_json::Value>,
    #[serde(rename = "track_type")]
    pub track_type: String,
    #[serde(default = "default_audio_player")]
    pub instrument: String,
    pub clips: Vec<MsqAudioClip>,
}

fn default_audio_player() -> String {
    "AudioPlayer".to_owned()
}

/// `MsqTrack`: a tagged union via `track_type`.
#[derive(Clone, Debug, PartialEq)]
pub enum MsqTrack {
    Singer(MsqSingerTrack),
    Audio(MsqAudioTrack),
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum MsqTrackRaw {
    Singer(MsqSingerTrack),
    Audio(MsqAudioTrack),
}

impl<'de> Deserialize<'de> for MsqTrack {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = MsqTrackRaw::deserialize(deserializer)?;
        Ok(match raw {
            MsqTrackRaw::Singer(track) => MsqTrack::Singer(track),
            MsqTrackRaw::Audio(track) => MsqTrack::Audio(track),
        })
    }
}

impl Serialize for MsqTrack {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MsqTrack::Singer(track) => track.serialize(serializer),
            MsqTrack::Audio(track) => track.serialize(serializer),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MikotoStudioSequenceFormat {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub setup: MsqSetup,
    pub main_out: MsqMainOut,
    #[serde(default = "default_aux")]
    pub aux: Vec<serde_json::Value>,
    #[serde(default = "default_tracks")]
    pub tracks: Vec<MsqTrack>,
    #[serde(default = "default_settings")]
    pub settings: MsqSettings,
    #[serde(default = "default_version")]
    pub version: i64,
}
