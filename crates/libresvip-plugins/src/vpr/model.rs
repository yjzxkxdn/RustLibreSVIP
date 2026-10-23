//! Port of `libresvip.plugins.vpr.model`.
//!
//! The structs keep the exact field order of the pydantic models because
//! `sequence.json` is written with `model_dump(mode="json", exclude_none=True,
//! by_alias=True)`: serde serialises struct fields in declaration order and
//! drops `None` fields, which reproduces pydantic's output byte for byte.

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `VocaloidLanguage` — pydantic serialises the integer value.
pub mod language {
    pub const JAPANESE: i64 = 0;
    pub const ENGLISH: i64 = 1;
    pub const KOREAN: i64 = 2;
    pub const SPANISH: i64 = 3;
    pub const SIMPLIFIED_CHINESE: i64 = 4;
}

/// `VocaloidTrackType`.
pub mod track_type {
    pub const STANDARD: i64 = 0;
    pub const AUDIO: i64 = 1;
    pub const AI: i64 = 2;
}

/// A `int | float | Literal["ZeroPitch"]` value.
///
/// Kept untagged so that JSON numbers keep their Python type: pydantic's
/// `mode="json"` dump writes an `int` field as an integer and a `float` field
/// with a `.0` suffix.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PointValue {
    Int(i64),
    Float(f64),
    Str(String),
}

impl PointValue {
    /// Python's `int(value)` for an int/float (truncation towards zero).
    pub fn as_py_int(&self) -> Option<i64> {
        match self {
            PointValue::Int(value) => Some(*value),
            PointValue::Float(value) => Some(*value as i64),
            PointValue::Str(_) => None,
        }
    }

    /// `value / BPM_RATE` for the numeric variants.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PointValue::Int(value) => Some(*value as f64),
            PointValue::Float(value) => Some(*value),
            PointValue::Str(_) => None,
        }
    }

    /// `isinstance(value, int | float)`.
    pub fn is_number(&self) -> bool {
        matches!(self, PointValue::Int(_) | PointValue::Float(_))
    }
}

// ---------------------------------------------------------------------------
// Small value models
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidBasePos {
    pub pos: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct VocaloidWithDur {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidPoint {
    pub pos: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<PointValue>,
}

impl VocaloidPoint {
    pub fn new(pos: i64, value: impl Into<PointValue>) -> Self {
        Self {
            pos,
            value: Some(value.into()),
        }
    }
}

impl From<i64> for PointValue {
    fn from(value: i64) -> Self {
        PointValue::Int(value)
    }
}

impl From<f64> for PointValue {
    fn from(value: f64) -> Self {
        PointValue::Float(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidTimeSig {
    #[serde(deserialize_with = "crate::vpr::model::lenient_i64")]
    pub bar: i64,
    #[serde(deserialize_with = "crate::vpr::model::lenient_i64")]
    pub denom: i64,
    #[serde(deserialize_with = "crate::vpr::model::lenient_i64")]
    pub numer: i64,
}

impl Default for VocaloidTimeSig {
    fn default() -> Self {
        Self {
            bar: 0,
            denom: 4,
            numer: 4,
        }
    }
}

// `deserialize_with` on numeric fields mimics pydantic's lenient validation
// (strings holding numbers are accepted).
pub fn lenient_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match &value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|float| float as i64))
            .ok_or_else(|| DeError::custom("invalid integer")),
        serde_json::Value::String(text) => text
            .trim()
            .parse::<i64>()
            .map_err(|error| DeError::custom(format!("invalid integer: {error}"))),
        serde_json::Value::Bool(flag) => Ok(i64::from(*flag)),
        serde_json::Value::Null => Ok(0),
        _ => Err(DeError::custom("invalid integer")),
    }
}

pub fn lenient_f64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match &value {
        serde_json::Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| DeError::custom("invalid float")),
        serde_json::Value::String(text) => text
            .trim()
            .parse::<f64>()
            .map_err(|error| D::Error::custom(format!("invalid float: {error}"))),
        serde_json::Value::Bool(flag) => Ok(f64::from(u8::from(*flag))),
        serde_json::Value::Null => Ok(0.0),
        _ => Err(D::Error::custom("invalid float")),
    }
}

pub fn lenient_bool<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match &value {
        serde_json::Value::Bool(flag) => Ok(*flag),
        serde_json::Value::Number(number) => Ok(number.as_f64().unwrap_or(0.0) != 0.0),
        serde_json::Value::String(text) => Ok(matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "y" | "on"
        )),
        serde_json::Value::Null => Ok(false),
        _ => Err(DeError::custom("invalid boolean")),
    }
}

pub fn lenient_opt_i64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    match &value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|float| float as i64))
            .map(Some)
            .ok_or_else(|| DeError::custom("invalid integer")),
        serde_json::Value::String(text) => text
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|error| DeError::custom(format!("invalid integer: {error}"))),
        serde_json::Value::Bool(flag) => Ok(Some(i64::from(*flag))),
        _ => Err(DeError::custom("invalid integer")),
    }
}

pub fn lenient_opt_f64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<f64>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    match &value {
        serde_json::Value::Number(number) => number
            .as_f64()
            .map(Some)
            .ok_or_else(|| DeError::custom("invalid float")),
        serde_json::Value::String(text) => text
            .trim()
            .parse::<f64>()
            .map(Some)
            .map_err(|error| DeError::custom(format!("invalid float: {error}"))),
        serde_json::Value::Bool(flag) => Ok(Some(f64::from(u8::from(*flag)))),
        _ => Err(DeError::custom("invalid float")),
    }
}

pub fn lenient_opt_bool<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    match &value {
        serde_json::Value::Bool(flag) => Ok(Some(*flag)),
        serde_json::Value::Number(number) => Ok(Some(number.as_f64().unwrap_or(0.0) != 0.0)),
        serde_json::Value::String(text) => Ok(Some(matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "y" | "on"
        ))),
        _ => Err(DeError::custom("invalid boolean")),
    }
}

// ---------------------------------------------------------------------------
// Expression / effect models
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidExp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decay: Option<i64>,
    #[serde(
        rename = "bendDepth",
        alias = "bend_depth",
        skip_serializing_if = "Option::is_none"
    )]
    pub bend_depth: Option<i64>,
    #[serde(
        rename = "bendLength",
        alias = "bend_length",
        skip_serializing_if = "Option::is_none"
    )]
    pub bend_length: Option<i64>,
}

impl Default for VocaloidExp {
    fn default() -> Self {
        Self {
            opening: Some(127),
            accent: None,
            decay: None,
            bend_depth: None,
            bend_length: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidAIExp {
    #[serde(rename = "pitchFine", alias = "pitch_fine", skip_serializing_if = "Option::is_none")]
    pub pitch_fine: Option<f64>,
    #[serde(
        rename = "pitchDriftStart",
        alias = "pitch_drift_start",
        skip_serializing_if = "Option::is_none"
    )]
    pub pitch_drift_start: Option<f64>,
    #[serde(
        rename = "pitchDriftEnd",
        alias = "pitch_drift_end",
        skip_serializing_if = "Option::is_none"
    )]
    pub pitch_drift_end: Option<f64>,
    #[serde(
        rename = "pitchScalingCenter",
        alias = "pitch_scaling_center",
        skip_serializing_if = "Option::is_none"
    )]
    pub pitch_scaling_center: Option<f64>,
    #[serde(
        rename = "pitchScalingOrigin",
        alias = "pitch_scaling_origin",
        skip_serializing_if = "Option::is_none"
    )]
    pub pitch_scaling_origin: Option<f64>,
    #[serde(
        rename = "pitchTransitionStart",
        alias = "pitch_transition_start",
        skip_serializing_if = "Option::is_none"
    )]
    pub pitch_transition_start: Option<f64>,
    #[serde(
        rename = "pitchTransitionEnd",
        alias = "pitch_transition_end",
        skip_serializing_if = "Option::is_none"
    )]
    pub pitch_transition_end: Option<f64>,
    #[serde(
        rename = "amplitudeWhole",
        alias = "amplitude_whole",
        skip_serializing_if = "Option::is_none"
    )]
    pub amplitude_whole: Option<f64>,
    #[serde(
        rename = "amplitudeStart",
        alias = "amplitude_start",
        skip_serializing_if = "Option::is_none"
    )]
    pub amplitude_start: Option<f64>,
    #[serde(
        rename = "amplitudeEnd",
        alias = "amplitude_end",
        skip_serializing_if = "Option::is_none"
    )]
    pub amplitude_end: Option<f64>,
    #[serde(
        rename = "formantWhole",
        alias = "formant_whole",
        skip_serializing_if = "Option::is_none"
    )]
    pub formant_whole: Option<f64>,
    #[serde(
        rename = "formantStart",
        alias = "formant_start",
        skip_serializing_if = "Option::is_none"
    )]
    pub formant_start: Option<f64>,
    #[serde(
        rename = "formantEnd",
        alias = "formant_end",
        skip_serializing_if = "Option::is_none"
    )]
    pub formant_end: Option<f64>,
    #[serde(
        rename = "vibratoLeadingDepth",
        alias = "vibrato_leading_depth",
        skip_serializing_if = "Option::is_none"
    )]
    pub vibrato_leading_depth: Option<f64>,
    #[serde(
        rename = "vibratoFollowingDepth",
        alias = "vibrato_following_depth",
        skip_serializing_if = "Option::is_none"
    )]
    pub vibrato_following_depth: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidEnabled {
    #[serde(
        rename = "isEnabled",
        alias = "is_enabled",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_enabled: Option<bool>,
}

impl Default for VocaloidEnabled {
    fn default() -> Self {
        Self {
            is_enabled: Some(true),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidGlobal {
    #[serde(
        rename = "isEnabled",
        alias = "is_enabled",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
}

impl Default for VocaloidGlobal {
    fn default() -> Self {
        Self {
            is_enabled: Some(true),
            value: Some(12000),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidRegion {
    #[serde(
        rename = "isEnabled",
        alias = "is_enabled",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_enabled: Option<bool>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub begin: Option<f64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub end: Option<f64>,
}

impl Default for VocaloidRegion {
    fn default() -> Self {
        Self {
            is_enabled: Some(true),
            begin: Some(0.0),
            end: Some(7680.0),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<PointValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidVersion {
    pub major: i64,
    pub minor: i64,
    pub revision: i64,
}

impl Default for VocaloidVersion {
    fn default() -> Self {
        Self {
            major: 5,
            minor: 0,
            revision: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidVibrato {
    #[serde(rename = "type", alias = "type_value")]
    pub type_value: i64,
    pub duration: i64,
    pub depths: Vec<VocaloidPoint>,
    pub rates: Vec<VocaloidPoint>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidCompID {
    #[serde(
        rename = "compID",
        alias = "comp_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub comp_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidLangID {
    #[serde(
        rename = "langID",
        alias = "lang_id",
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub lang_id: Option<i64>,
}

impl Default for VocaloidLangID {
    fn default() -> Self {
        Self {
            lang_id: Some(language::SIMPLIFIED_CHINESE),
        }
    }
}

/// `class VocaloidVoice(VocaloidCompID, VocaloidLangID)` — pydantic collects
/// the fields in reverse MRO order, so `langID` comes before `compID`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidVoice {
    #[serde(
        rename = "langID",
        alias = "lang_id",
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub lang_id: Option<i64>,
    #[serde(
        rename = "compID",
        alias = "comp_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub comp_id: Option<String>,
}

impl Default for VocaloidVoice {
    fn default() -> Self {
        Self {
            lang_id: Some(language::SIMPLIFIED_CHINESE),
            comp_id: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidAIVoice {
    #[serde(
        rename = "compID",
        alias = "comp_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub comp_id: Option<String>,
    #[serde(rename = "langIDs", alias = "lang_ids")]
    pub lang_ids: Vec<VocaloidLangID>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidVoices {
    #[serde(
        rename = "compID",
        alias = "comp_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub comp_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidWeight {
    pub pre: i64,
    pub post: i64,
}

impl Default for VocaloidWeight {
    fn default() -> Self {
        Self { pre: 64, post: 64 }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidControllers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub events: Vec<VocaloidPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidFolded {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
}

impl Default for VocaloidFolded {
    fn default() -> Self {
        Self {
            is_folded: Some(true),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidEffects {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
    #[serde(rename = "id", alias = "id_value", skip_serializing_if = "Option::is_none")]
    pub id_value: Option<String>,
    #[serde(rename = "isBypassed", alias = "is_bypassed", skip_serializing_if = "Option::is_none")]
    pub is_bypassed: Option<bool>,
    pub parameters: Vec<VocaloidParameters>,
}

impl VocaloidEffects {
    pub fn folded() -> Self {
        Self {
            is_folded: Some(true),
            id_value: None,
            is_bypassed: None,
            parameters: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidSingingSkill {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<VocaloidWeight>,
}

impl Default for VocaloidSingingSkill {
    fn default() -> Self {
        Self {
            duration: Some(158),
            weight: Some(VocaloidWeight::default()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidTempo {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
    pub events: Vec<VocaloidPoint>,
    #[serde(
        rename = "global",
        alias = "global_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub global: Option<VocaloidGlobal>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ara: Option<VocaloidEnabled>,
}

impl Default for VocaloidTempo {
    fn default() -> Self {
        Self {
            is_folded: Some(true),
            events: Vec::new(),
            global: Some(VocaloidGlobal::default()),
            height: Some(0.0),
            ara: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidTimeSigs {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
    pub events: Vec<VocaloidTimeSig>,
}

impl Default for VocaloidTimeSigs {
    fn default() -> Self {
        Self {
            is_folded: Some(true),
            events: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidAutomation {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub height: Option<f64>,
    pub events: Vec<VocaloidPoint>,
}

impl Default for VocaloidAutomation {
    fn default() -> Self {
        Self {
            is_folded: Some(true),
            height: Some(0.0),
            events: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidMasterTrack {
    #[serde(rename = "loop", alias = "loop_")]
    pub loop_: VocaloidRegion,
    #[serde(
        rename = "samplingRate",
        alias = "sampling_rate",
        deserialize_with = "crate::vpr::model::lenient_i64"
    )]
    pub sampling_rate: i64,
    pub tempo: VocaloidTempo,
    #[serde(rename = "timeSig", alias = "time_sig")]
    pub time_sig: VocaloidTimeSigs,
    pub volume: VocaloidAutomation,
    #[serde(
        rename = "mainTuning",
        alias = "main_tuning",
        deserialize_with = "crate::vpr::model::lenient_f64"
    )]
    pub main_tuning: f64,
}

impl Default for VocaloidMasterTrack {
    fn default() -> Self {
        Self {
            loop_: VocaloidRegion::default(),
            sampling_rate: 44100,
            tempo: VocaloidTempo::default(),
            time_sig: VocaloidTimeSigs::default(),
            volume: VocaloidAutomation::default(),
            main_tuning: 440.0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidDVQMRelease {
    #[serde(rename = "compID", alias = "comp_id", skip_serializing_if = "Option::is_none")]
    pub comp_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<i64>,
    #[serde(rename = "levelNames", alias = "level_names", skip_serializing_if = "Option::is_none")]
    pub level_names: Option<Vec<String>>,
    #[serde(rename = "topFactor", alias = "top_factor", skip_serializing_if = "Option::is_none")]
    pub top_factor: Option<f64>,
    #[serde(rename = "isProtected", alias = "is_protected", skip_serializing_if = "Option::is_none")]
    pub is_protected: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidDVQM {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<VocaloidDVQMRelease>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack: Option<VocaloidDVQMRelease>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidNotes {
    #[serde(deserialize_with = "crate::vpr::model::lenient_i64")]
    pub pos: i64,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration: Option<i64>,
    #[serde(
        rename = "langID",
        alias = "lang_id",
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub lang_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<VocaloidExp>,
    #[serde(rename = "aiExp", alias = "ai_exp", skip_serializing_if = "Option::is_none")]
    pub ai_exp: Option<VocaloidAIExp>,
    #[serde(
        rename = "directPitches",
        alias = "direct_pitches",
        skip_serializing_if = "Option::is_none"
    )]
    pub direct_pitches: Option<Vec<VocaloidPoint>>,
    #[serde(
        rename = "isProtected",
        alias = "is_protected",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_protected: Option<bool>,
    #[serde(
        rename = "isAiVibratoEnabled",
        alias = "is_ai_vibrato_enabled",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_ai_vibrato_enabled: Option<bool>,
    pub lyric: String,
    #[serde(deserialize_with = "crate::vpr::model::lenient_i64")]
    pub number: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phoneme: Option<String>,
    #[serde(
        rename = "phonemePositions",
        alias = "phoneme_positions",
        skip_serializing_if = "Option::is_none"
    )]
    pub phoneme_positions: Option<Vec<VocaloidBasePos>>,
    #[serde(
        rename = "singingSkill",
        alias = "singing_skill",
        skip_serializing_if = "Option::is_none"
    )]
    pub singing_skill: Option<VocaloidSingingSkill>,
    #[serde(deserialize_with = "crate::vpr::model::lenient_i64")]
    pub velocity: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibrato: Option<VocaloidVibrato>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dvqm: Option<VocaloidDVQM>,
}

impl Default for VocaloidNotes {
    fn default() -> Self {
        Self {
            pos: 0,
            duration: None,
            lang_id: Some(language::SIMPLIFIED_CHINESE),
            exp: Some(VocaloidExp::default()),
            ai_exp: None,
            direct_pitches: None,
            is_protected: Some(false),
            is_ai_vibrato_enabled: Some(false),
            lyric: String::new(),
            number: 0,
            phoneme: None,
            phoneme_positions: None,
            singing_skill: Some(VocaloidSingingSkill::default()),
            velocity: 64,
            vibrato: Some(VocaloidVibrato::default()),
            dvqm: None,
        }
    }
}

impl VocaloidNotes {
    /// The subset of fields the generator writes, in model order.
    pub fn new(
        pos: i64,
        duration: i64,
        number: i64,
        lyric: String,
        phoneme: Option<String>,
        lang_id: i64,
    ) -> Self {
        Self {
            pos,
            duration: Some(duration),
            lang_id: Some(lang_id),
            lyric,
            number,
            phoneme,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidWav {
    pub name: String,
    #[serde(
        rename = "originalName",
        alias = "original_name",
        skip_serializing_if = "Option::is_none"
    )]
    pub original_name: Option<String>,
}

/// Common prefix of `VocaloidVoicePart` / `VocaloidWavPart` (reverse MRO:
/// `pos`, `duration`, `name`, `audioEffects`).
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct VocaloidBasePart {
    pub pos: i64,
    pub duration: Option<i64>,
    pub name: Option<String>,
    pub audio_effects: Vec<VocaloidEffects>,
}

impl VocaloidBasePart {
    pub fn new(pos: i64, duration: Option<i64>) -> Self {
        Self {
            pos,
            duration,
            name: Some(String::new()),
            audio_effects: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidVoicePart {
    #[serde(deserialize_with = "crate::vpr::model::lenient_i64")]
    pub pos: i64,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "audioEffects", alias = "audio_effects")]
    pub audio_effects: Vec<VocaloidEffects>,
    #[serde(rename = "midiEffects", alias = "midi_effects")]
    pub midi_effects: Vec<VocaloidEffects>,
    pub notes: Vec<VocaloidNotes>,
    #[serde(
        rename = "stylePresetID",
        alias = "style_preset_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub style_preset_id: Option<String>,
    #[serde(rename = "styleName", alias = "style_name", skip_serializing_if = "Option::is_none")]
    pub style_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<VocaloidVoice>,
    #[serde(
        rename = "secondaryVoice",
        alias = "secondary_voice",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_voice: Option<VocaloidVoice>,
    #[serde(rename = "aiVoice", alias = "ai_voice", skip_serializing_if = "Option::is_none")]
    pub ai_voice: Option<VocaloidAIVoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controllers: Option<Vec<VocaloidControllers>>,
}

impl Default for VocaloidVoicePart {
    fn default() -> Self {
        Self {
            pos: 0,
            duration: None,
            name: Some(String::new()),
            audio_effects: Vec::new(),
            midi_effects: Vec::new(),
            notes: Vec::new(),
            style_preset_id: None,
            style_name: Some("No Effect".to_owned()),
            voice: None,
            secondary_voice: None,
            ai_voice: None,
            controllers: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidWavPart {
    #[serde(deserialize_with = "crate::vpr::model::lenient_i64")]
    pub pos: i64,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "audioEffects", alias = "audio_effects")]
    pub audio_effects: Vec<VocaloidEffects>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<VocaloidRegion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wav: Option<VocaloidWav>,
    #[serde(
        rename = "voiceChanger",
        alias = "voice_changer",
        skip_serializing_if = "Option::is_none"
    )]
    pub voice_changer: Option<VocaloidCompID>,
}

impl Default for VocaloidWavPart {
    fn default() -> Self {
        Self {
            pos: 0,
            duration: None,
            name: Some(String::new()),
            audio_effects: Vec::new(),
            region: None,
            wav: None,
            voice_changer: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct VocaloidBaseTracks {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
    #[serde(
        rename = "busNo",
        alias = "bus_no",
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub bus_no: Option<i64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<i64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub height: Option<f64>,
    #[serde(
        rename = "isMuted",
        alias = "is_muted",
        deserialize_with = "crate::vpr::model::lenient_bool"
    )]
    pub is_muted: bool,
    #[serde(
        rename = "isSoloMode",
        alias = "is_solo_mode",
        deserialize_with = "crate::vpr::model::lenient_bool"
    )]
    pub is_solo_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub panpot: VocaloidAutomation,
    pub volume: VocaloidAutomation,
    #[serde(
        rename = "lastScrollPositionNoteNumber",
        alias = "last_scroll_position_note_number",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_scroll_position_note_number: Option<i64>,
    #[serde(rename = "audioEffects", alias = "audio_effects")]
    pub audio_effects: Vec<VocaloidEffects>,
}

impl Default for VocaloidBaseTracks {
    fn default() -> Self {
        Self {
            is_folded: Some(true),
            bus_no: Some(0),
            color: Some(0),
            height: Some(0.0),
            is_muted: false,
            is_solo_mode: false,
            name: Some(String::new()),
            panpot: VocaloidAutomation::default(),
            volume: VocaloidAutomation::default(),
            last_scroll_position_note_number: None,
            audio_effects: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidStandardTrack {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
    #[serde(
        rename = "busNo",
        alias = "bus_no",
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub bus_no: Option<i64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<i64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub height: Option<f64>,
    #[serde(
        rename = "isMuted",
        alias = "is_muted",
        deserialize_with = "crate::vpr::model::lenient_bool"
    )]
    pub is_muted: bool,
    #[serde(
        rename = "isSoloMode",
        alias = "is_solo_mode",
        deserialize_with = "crate::vpr::model::lenient_bool"
    )]
    pub is_solo_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub panpot: VocaloidAutomation,
    pub volume: VocaloidAutomation,
    #[serde(
        rename = "lastScrollPositionNoteNumber",
        alias = "last_scroll_position_note_number",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_scroll_position_note_number: Option<i64>,
    #[serde(rename = "audioEffects", alias = "audio_effects")]
    pub audio_effects: Vec<VocaloidEffects>,
    pub parts: Vec<VocaloidVoicePart>,
    #[serde(rename = "type", alias = "type_value")]
    pub type_value: i64,
}

impl Default for VocaloidStandardTrack {
    fn default() -> Self {
        Self {
            is_folded: Some(true),
            bus_no: Some(0),
            color: Some(0),
            height: Some(0.0),
            is_muted: false,
            is_solo_mode: false,
            name: Some(String::new()),
            panpot: VocaloidAutomation::default(),
            volume: VocaloidAutomation::default(),
            last_scroll_position_note_number: None,
            audio_effects: Vec::new(),
            parts: Vec::new(),
            type_value: track_type::STANDARD,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidAITrack {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
    #[serde(
        rename = "busNo",
        alias = "bus_no",
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub bus_no: Option<i64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<i64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub height: Option<f64>,
    #[serde(
        rename = "isMuted",
        alias = "is_muted",
        deserialize_with = "crate::vpr::model::lenient_bool"
    )]
    pub is_muted: bool,
    #[serde(
        rename = "isSoloMode",
        alias = "is_solo_mode",
        deserialize_with = "crate::vpr::model::lenient_bool"
    )]
    pub is_solo_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub panpot: VocaloidAutomation,
    pub volume: VocaloidAutomation,
    #[serde(
        rename = "lastScrollPositionNoteNumber",
        alias = "last_scroll_position_note_number",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_scroll_position_note_number: Option<i64>,
    #[serde(rename = "audioEffects", alias = "audio_effects")]
    pub audio_effects: Vec<VocaloidEffects>,
    pub parts: Vec<VocaloidVoicePart>,
    #[serde(rename = "type", alias = "type_value")]
    pub type_value: i64,
}

impl Default for VocaloidAITrack {
    fn default() -> Self {
        Self {
            is_folded: Some(true),
            bus_no: Some(0),
            color: Some(0),
            height: Some(0.0),
            is_muted: false,
            is_solo_mode: false,
            name: Some(String::new()),
            panpot: VocaloidAutomation::default(),
            volume: VocaloidAutomation::default(),
            last_scroll_position_note_number: None,
            audio_effects: Vec::new(),
            parts: Vec::new(),
            type_value: track_type::AI,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidAudioTrack {
    #[serde(
        rename = "isFolded",
        alias = "is_folded",
        deserialize_with = "crate::vpr::model::lenient_opt_bool",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_folded: Option<bool>,
    #[serde(
        rename = "busNo",
        alias = "bus_no",
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub bus_no: Option<i64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_i64",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<i64>,
    #[serde(
        deserialize_with = "crate::vpr::model::lenient_opt_f64",
        skip_serializing_if = "Option::is_none"
    )]
    pub height: Option<f64>,
    #[serde(
        rename = "isMuted",
        alias = "is_muted",
        deserialize_with = "crate::vpr::model::lenient_bool"
    )]
    pub is_muted: bool,
    #[serde(
        rename = "isSoloMode",
        alias = "is_solo_mode",
        deserialize_with = "crate::vpr::model::lenient_bool"
    )]
    pub is_solo_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub panpot: VocaloidAutomation,
    pub volume: VocaloidAutomation,
    #[serde(
        rename = "lastScrollPositionNoteNumber",
        alias = "last_scroll_position_note_number",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_scroll_position_note_number: Option<i64>,
    #[serde(rename = "audioEffects", alias = "audio_effects")]
    pub audio_effects: Vec<VocaloidEffects>,
    pub parts: Vec<VocaloidWavPart>,
    #[serde(rename = "type", alias = "type_value")]
    pub type_value: i64,
}

impl Default for VocaloidAudioTrack {
    fn default() -> Self {
        Self {
            is_folded: Some(true),
            bus_no: Some(0),
            color: Some(0),
            height: Some(0.0),
            is_muted: false,
            is_solo_mode: false,
            name: Some(String::new()),
            panpot: VocaloidAutomation::default(),
            volume: VocaloidAutomation::default(),
            last_scroll_position_note_number: None,
            audio_effects: Vec::new(),
            parts: Vec::new(),
            type_value: track_type::AUDIO,
        }
    }
}

/// `VocaloidTracks` — the discriminated union on `type`.
#[derive(Clone, Debug)]
pub enum VocaloidTrack {
    Standard(VocaloidStandardTrack),
    Ai(VocaloidAITrack),
    Audio(VocaloidAudioTrack),
}

impl VocaloidTrack {
    pub fn name(&self) -> Option<&str> {
        match self {
            VocaloidTrack::Standard(track) => track.name.as_deref(),
            VocaloidTrack::Ai(track) => track.name.as_deref(),
            VocaloidTrack::Audio(track) => track.name.as_deref(),
        }
    }

    pub fn is_muted(&self) -> bool {
        match self {
            VocaloidTrack::Standard(track) => track.is_muted,
            VocaloidTrack::Ai(track) => track.is_muted,
            VocaloidTrack::Audio(track) => track.is_muted,
        }
    }

    pub fn is_solo_mode(&self) -> bool {
        match self {
            VocaloidTrack::Standard(track) => track.is_solo_mode,
            VocaloidTrack::Ai(track) => track.is_solo_mode,
            VocaloidTrack::Audio(track) => track.is_solo_mode,
        }
    }
}

impl Serialize for VocaloidTrack {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            VocaloidTrack::Standard(track) => track.serialize(serializer),
            VocaloidTrack::Ai(track) => track.serialize(serializer),
            VocaloidTrack::Audio(track) => track.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for VocaloidTrack {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let type_value = value
            .get("type")
            .and_then(|item| item.as_i64())
            .unwrap_or(track_type::STANDARD);
        match type_value {
            track_type::STANDARD => serde_json::from_value(value)
                .map(VocaloidTrack::Standard)
                .map_err(|error| D::Error::custom(format!("invalid standard track: {error}"))),
            track_type::AI => serde_json::from_value(value)
                .map(VocaloidTrack::Ai)
                .map_err(|error| D::Error::custom(format!("invalid AI track: {error}"))),
            track_type::AUDIO => serde_json::from_value(value)
                .map(VocaloidTrack::Audio)
                .map_err(|error| D::Error::custom(format!("invalid audio track: {error}"))),
            other => Err(D::Error::custom(format!("invalid track type: {other}"))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VocaloidProject {
    #[serde(rename = "masterTrack", alias = "master_track")]
    pub master_track: VocaloidMasterTrack,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub tracks: Vec<VocaloidTrack>,
    pub vender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<VocaloidVersion>,
    pub voices: Vec<VocaloidVoices>,
}

impl Default for VocaloidProject {
    fn default() -> Self {
        Self {
            master_track: VocaloidMasterTrack::default(),
            title: Some(String::new()),
            tracks: Vec::new(),
            vender: "Yamaha Corporation".to_owned(),
            version: Some(VocaloidVersion::default()),
            voices: Vec::new(),
        }
    }
}
