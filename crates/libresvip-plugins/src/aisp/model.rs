//! Pydantic models of the AISingers project format.
//!
//! Only deserialisation is derived: dumping is done by the generator, which
//! writes the JSON by hand so that field order, computed fields
//! (`signature[].str`, `num_track`) and Python's float spelling match exactly.

use serde::{Deserialize, Deserializer};

fn default_flags() -> Option<i64> {
    Some(-256)
}

fn default_flage() -> Option<i64> {
    Some(-128)
}

fn default_vel() -> Option<i64> {
    Some(50)
}

fn default_zero() -> Option<i64> {
    Some(0)
}

fn default_two() -> Option<i64> {
    Some(2)
}

fn default_beat_zi() -> i64 {
    4
}

fn default_beat_mu() -> i64 {
    4
}

fn default_singer_text() -> Option<String> {
    Some(String::new())
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AisProjectHead {
    pub tempo: Vec<AisTempo>,
    pub signature: Vec<AisTimeSignature>,
    #[serde(default = "default_flags")]
    pub flags: Option<i64>,
    #[serde(default = "default_flage")]
    pub flage: Option<i64>,
    pub time: Option<i64>,
    pub bar: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AisTempo {
    pub tempo_float: Option<f64>,
    pub start_128: i64,
    pub start_bar: i64,
    pub start_beat_in_bar: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AisTimeSignature {
    #[serde(default = "default_beat_zi")]
    pub beat_zi: i64,
    #[serde(default = "default_beat_mu")]
    pub beat_mu: i64,
    pub start_bar: i64,
}

impl Default for AisTimeSignature {
    fn default() -> Self {
        Self {
            beat_zi: 4,
            beat_mu: 4,
            start_bar: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AisProjectBody {
    pub tracks: Vec<AisTrack>,
}

#[derive(Clone, Debug)]
pub enum AisTrack {
    Sing(AisSingVoiceTrack),
    Audio(AisAudioTrack),
    Midi(AisMidiTrack),
}

pub const TRACK_SING_VOICE: i64 = 0;
pub const TRACK_AUDIO: i64 = 1;
pub const TRACK_MIDI: i64 = 2;

fn track_type(value: &serde_json::Value) -> Option<i64> {
    match value.get("t")? {
        serde_json::Value::Number(number) => number.as_i64(),
        serde_json::Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

impl<'de> Deserialize<'de> for AisTrack {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = serde_json::Value::deserialize(deserializer)?;
        match track_type(&value) {
            Some(TRACK_SING_VOICE) => serde_json::from_value(value)
                .map(AisTrack::Sing)
                .map_err(|error| D::Error::custom(format!("invalid singing track: {error}"))),
            Some(TRACK_AUDIO) => serde_json::from_value(value)
                .map(AisTrack::Audio)
                .map_err(|error| D::Error::custom(format!("invalid audio track: {error}"))),
            Some(TRACK_MIDI) => serde_json::from_value(value)
                .map(AisTrack::Midi)
                .map_err(|error| D::Error::custom(format!("invalid midi track: {error}"))),
            Some(other) => Err(D::Error::custom(format!("unknown track type: {other}"))),
            None => Err(D::Error::missing_field("t")),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AisBaseTrack {
    #[serde(rename = "i")]
    pub idx: Option<i64>,
    #[serde(rename = "s")]
    pub solo: Option<bool>,
    #[serde(rename = "m")]
    pub mute: Option<bool>,
    #[serde(rename = "v")]
    pub volume: Option<f64>,
    #[serde(rename = "n")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AisSingVoiceTrack {
    #[serde(flatten)]
    pub base: AisBaseTrack,
    #[serde(rename = "sn")]
    pub singer_namecn: Option<String>,
    #[serde(rename = "se", default = "default_singer_text")]
    pub singer_nameen: Option<String>,
    #[serde(rename = "sh", default = "default_singer_text")]
    pub singer_head_path: Option<String>,
    #[serde(rename = "im")]
    pub items: Vec<AisSingVoicePattern>,
}

impl Default for AisSingVoiceTrack {
    fn default() -> Self {
        Self {
            base: AisBaseTrack::default(),
            singer_namecn: None,
            singer_nameen: Some(String::new()),
            singer_head_path: Some(String::new()),
            items: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AisAudioTrack {
    #[serde(flatten)]
    pub base: AisBaseTrack,
    #[serde(rename = "im")]
    pub items: Vec<AisAudioPattern>,
}

impl Default for AisAudioTrack {
    fn default() -> Self {
        Self {
            base: AisBaseTrack::default(),
            items: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AisMidiTrack {
    #[serde(flatten)]
    pub base: AisBaseTrack,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AisSingVoicePattern {
    pub uid: Option<i64>,
    #[serde(rename = "s")]
    pub start: i64,
    #[serde(rename = "l")]
    pub length: Option<i64>,
    #[serde(rename = "n")]
    pub notes: Vec<AisNote>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AisAudioPattern {
    pub uid: Option<i64>,
    #[serde(rename = "s")]
    pub start: i64,
    #[serde(rename = "l")]
    pub length: Option<i64>,
    #[serde(rename = "pa")]
    pub path_audio: Option<String>,
    #[serde(rename = "pw")]
    pub path_wave: Option<String>,
    #[serde(default = "default_two")]
    pub n_channel: Option<i64>,
    #[serde(default = "default_zero")]
    pub len_sec: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AisNote {
    #[serde(rename = "s")]
    pub start: i64,
    #[serde(rename = "l")]
    pub length: i64,
    #[serde(rename = "m")]
    pub midi_no: i64,
    #[serde(rename = "ly")]
    pub lyric: Option<String>,
    #[serde(rename = "py")]
    pub pinyin: Option<String>,
    #[serde(default = "default_vel")]
    pub vel: Option<i64>,
    #[serde(rename = "tri", default)]
    pub triple: bool,
    #[serde(default, deserialize_with = "deserialize_pit")]
    pub pit: Vec<f64>,
    #[serde(default = "default_zero")]
    pub bc: Option<i64>,
    #[serde(default = "default_zero")]
    pub bj: Option<i64>,
    #[serde(default = "default_zero")]
    pub bq: Option<i64>,
}

impl Default for AisNote {
    fn default() -> Self {
        Self {
            start: 0,
            length: 0,
            midi_no: 0,
            lyric: None,
            pinyin: None,
            vel: Some(50),
            triple: false,
            pit: Vec::new(),
            bc: Some(0),
            bj: Some(0),
            bq: Some(0),
        }
    }
}

fn push_pit_token(pit_list: &mut Vec<f64>, token: &str) {
    if let Some(position) = token.find('x') {
        let value = &token[..position];
        let repeat = &token[position + 1..];
        if let (Ok(parsed), Ok(count)) = (value.parse::<f64>(), repeat.parse::<usize>()) {
            for _ in 0..count {
                pit_list.push(parsed);
            }
        }
    } else if let Ok(parsed) = token.parse::<f64>() {
        pit_list.push(parsed);
    }
}

fn deserialize_pit<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<f64>, D::Error> {
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    let mut pit_list = Vec::new();
    match value {
        serde_json::Value::Null => return Ok(pit_list),
        serde_json::Value::String(text) => {
            for token in text.split_whitespace() {
                push_pit_token(&mut pit_list, token);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                match item {
                    serde_json::Value::String(text) => {
                        for token in text.split_whitespace() {
                            push_pit_token(&mut pit_list, token);
                        }
                    }
                    serde_json::Value::Number(number) => {
                        if let Some(value) = number.as_f64() {
                            pit_list.push(value);
                        }
                    }
                    other => {
                        return Err(D::Error::custom(format!(
                            "invalid pitch sample: {other}"
                        )));
                    }
                }
            }
        }
        other => return Err(D::Error::custom(format!("invalid pit value: {other}"))),
    }
    Ok(pit_list)
}
