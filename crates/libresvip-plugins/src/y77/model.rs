//! Yuan77 project model (`y77`), port of `libresvip.plugins.y77.model`.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

pub const DEFAULT_BPM: f64 = 120.0;

/// `int | float` — Python serialises the two differently (`50` vs `50.0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PitValue {
    Int(i64),
    Float(f64),
}

impl PitValue {
    pub fn as_f64(self) -> f64 {
        match self {
            PitValue::Int(value) => value as f64,
            PitValue::Float(value) => value,
        }
    }
}

impl Serialize for PitValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            PitValue::Int(value) => serializer.serialize_i64(*value),
            PitValue::Float(value) => serializer.serialize_f64(*value),
        }
    }
}

impl<'de> Deserialize<'de> for PitValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PitVisitor;

        impl<'de> Visitor<'de> for PitVisitor {
            type Value = PitValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an integer or float")
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<PitValue, E> {
                Ok(PitValue::Int(value))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<PitValue, E> {
                Ok(PitValue::Int(value as i64))
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<PitValue, E> {
                Ok(PitValue::Float(value))
            }
        }

        deserializer.deserialize_any(PitVisitor)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Y77Note {
    #[serde(default)]
    pub py: String,
    #[serde(rename = "len", alias = "length")]
    pub length: i64,
    pub start: i64,
    pub lyric: String,
    pub pitch: i64,
    #[serde(default)]
    pub pbs: i64,
    #[serde(default)]
    pub pit: Vec<PitValue>,
}

impl Default for Y77Note {
    fn default() -> Self {
        Self {
            py: String::new(),
            length: 0,
            start: 0,
            lyric: String::new(),
            pitch: 0,
            pbs: 0,
            pit: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Y77Project {
    #[serde(default = "default_bpm")]
    pub bpm: f64,
    #[serde(default)]
    pub bars: i64,
    #[serde(default)]
    pub notes: Vec<Y77Note>,
    #[serde(default)]
    pub nnote: i64,
    #[serde(default = "default_bbar")]
    pub bbar: i64,
    #[serde(default = "default_v")]
    pub v: i64,
    #[serde(default = "default_bbeat")]
    pub bbeat: i64,
}

fn default_bpm() -> f64 {
    DEFAULT_BPM
}

fn default_bbar() -> i64 {
    4
}

fn default_v() -> i64 {
    10001
}

fn default_bbeat() -> i64 {
    4
}

impl Default for Y77Project {
    fn default() -> Self {
        Self {
            bpm: DEFAULT_BPM,
            bars: 0,
            notes: Vec::new(),
            nnote: 0,
            bbar: 4,
            v: 10001,
            bbeat: 4,
        }
    }
}

/// `json.dumps(..., ensure_ascii=True)` escapes every non-ASCII code point as
/// `\uXXXX` (lowercase hex, surrogate pairs for astral characters).
pub fn escape_non_ascii(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii() {
            output.push(character);
        } else {
            let mut buffer = [0u16; 2];
            for unit in character.encode_utf16(&mut buffer) {
                output.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    output
}

