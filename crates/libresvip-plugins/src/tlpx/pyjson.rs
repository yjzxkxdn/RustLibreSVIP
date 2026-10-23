//! Ordered JSON/CBOR tree for the `tlpx` plugin.
//!
//! The tree keeps insertion order (pydantic emits fields in declaration order)
//! and is turned into CBOR with `cbor2`'s defaults: definite-length maps and
//! arrays, the smallest integer width, 64-bit floats for every `float` field.
//! A Python-compatible JSON writer is included as well (used by the other
//! plugins of this family; keep the two copies in sync).

use libresvip_core::error::{LibreSvipError, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Default for Json {
    /// pydantic's `dict[str, Any] = Field(default_factory=dict)` counterpart.
    fn default() -> Self {
        Json::Object(Vec::new())
    }
}

impl Json {
    pub fn empty_object() -> Self {
        Json::Object(Vec::new())
    }

    pub fn object<const N: usize>(pairs: [(&str, Json); N]) -> Self {
        Json::Object(
            pairs
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(entries) => entries
                .iter()
                .find(|(entry_key, _)| entry_key == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Int(value) => Some(*value),
            Json::Float(value) if value.fract() == 0.0 => Some(*value as i64),
            Json::Str(value) => value.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Int(value) => Some(*value as f64),
            Json::Float(value) => Some(*value),
            Json::Str(value) => value.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
}

/// Parses JSON text into the ordered tree (serde_json keeps object order).
pub fn parse(text: &str) -> Result<Json> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    Ok(from_serde(value))
}

fn from_serde(value: serde_json::Value) -> Json {
    match value {
        serde_json::Value::Null => Json::Null,
        serde_json::Value::Bool(value) => Json::Bool(value),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(value) => Json::Int(value),
            None => Json::Float(number.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::String(value) => Json::Str(value),
        serde_json::Value::Array(items) => Json::Array(items.into_iter().map(from_serde).collect()),
        serde_json::Value::Object(entries) => Json::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key, from_serde(value)))
                .collect(),
        ),
    }
}

// ---------------------------------------------------------------------------
// CBOR
// ---------------------------------------------------------------------------

fn cbor_error(error: impl std::fmt::Display) -> LibreSvipError {
    LibreSvipError::Validation(format!("invalid CBOR: {error}"))
}

/// `cbor2.dumps` of the tree.
///
/// cbor2 always writes floats as 64-bit doubles and uses the shortest integer
/// width; ciborium would shrink doubles to f16/f32, so the encoder is manual.
pub fn encode_cbor(value: &Json) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    write_cbor(value, &mut output)?;
    Ok(output)
}

/// `cbor2.loads` into the tree.
pub fn decode_cbor(bytes: &[u8]) -> Result<Json> {
    let value: ciborium::value::Value =
        ciborium::de::from_reader(bytes).map_err(cbor_error)?;
    from_cbor(value)
}

fn write_cbor_head(output: &mut Vec<u8>, major: u8, value: u64) {
    let prefix = major << 5;
    if value < 24 {
        output.push(prefix | value as u8);
    } else if value < 0x100 {
        output.push(prefix | 24);
        output.push(value as u8);
    } else if value < 0x1_0000 {
        output.push(prefix | 25);
        output.extend_from_slice(&(value as u16).to_be_bytes());
    } else if value < 0x1_0000_0000 {
        output.push(prefix | 26);
        output.extend_from_slice(&(value as u32).to_be_bytes());
    } else {
        output.push(prefix | 27);
        output.extend_from_slice(&value.to_be_bytes());
    }
}

fn write_cbor(value: &Json, output: &mut Vec<u8>) -> Result<()> {
    match value {
        Json::Null => output.push(0xf6),
        Json::Bool(true) => output.push(0xf5),
        Json::Bool(false) => output.push(0xf4),
        Json::Int(value) => {
            if *value >= 0 {
                write_cbor_head(output, 0, *value as u64);
            } else {
                write_cbor_head(output, 1, (-1i128 - *value as i128) as u64);
            }
        }
        Json::Float(value) => {
            output.push(0xfb);
            output.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        Json::Str(value) => {
            write_cbor_head(output, 3, value.len() as u64);
            output.extend_from_slice(value.as_bytes());
        }
        Json::Array(items) => {
            write_cbor_head(output, 4, items.len() as u64);
            for item in items {
                write_cbor(item, output)?;
            }
        }
        Json::Object(entries) => {
            write_cbor_head(output, 5, entries.len() as u64);
            for (key, item) in entries {
                write_cbor_head(output, 3, key.len() as u64);
                output.extend_from_slice(key.as_bytes());
                write_cbor(item, output)?;
            }
        }
    }
    Ok(())
}

pub fn from_cbor(value: ciborium::value::Value) -> Result<Json> {
    use ciborium::value::Value;
    Ok(match value {
        Value::Null => Json::Null,
        Value::Bool(value) => Json::Bool(value),
        Value::Integer(value) => {
            let raw: i128 = value.into();
            match i64::try_from(raw) {
                Ok(value) => Json::Int(value),
                Err(_) => Json::Float(raw as f64),
            }
        }
        Value::Float(value) => Json::Float(value),
        Value::Text(value) => Json::Str(value),
        Value::Bytes(value) => Json::Array(
            value
                .into_iter()
                .map(|byte| Json::Int(byte as i64))
                .collect(),
        ),
        Value::Array(items) => Json::Array(
            items
                .into_iter()
                .map(from_cbor)
                .collect::<Result<Vec<_>>>()?,
        ),
        Value::Map(entries) => Json::Object(
            entries
                .into_iter()
                .map(|(key, value)| -> Result<(String, Json)> {
                    let key = match key {
                        Value::Text(value) => value,
                        other => {
                            return Err(LibreSvipError::Validation(format!(
                                "unsupported CBOR map key: {other:?}"
                            )))
                        }
                    };
                    Ok((key, from_cbor(value)?))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        Value::Tag(_, inner) => from_cbor(*inner)?,
        other => {
            return Err(LibreSvipError::Validation(format!(
                "unsupported CBOR value: {other:?}"
            )))
        }
    })
}

// ---------------------------------------------------------------------------
// JSON writer
// ---------------------------------------------------------------------------

/// `json.dumps(value, ensure_ascii=..., separators=...)`.
pub fn dumps(value: &Json, ensure_ascii: bool, compact: bool) -> String {
    let mut output = String::new();
    write_value(value, &mut output, ensure_ascii, compact);
    output
}

fn write_value(value: &Json, output: &mut String, ensure_ascii: bool, compact: bool) {
    match value {
        Json::Null => output.push_str("null"),
        Json::Bool(true) => output.push_str("true"),
        Json::Bool(false) => output.push_str("false"),
        Json::Int(value) => output.push_str(&value.to_string()),
        Json::Float(value) => output.push_str(&format_float(*value)),
        Json::Str(value) => write_string(value, output, ensure_ascii),
        Json::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                    if !compact {
                        output.push(' ');
                    }
                }
                write_value(item, output, ensure_ascii, compact);
            }
            output.push(']');
        }
        Json::Object(entries) => {
            output.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                    if !compact {
                        output.push(' ');
                    }
                }
                write_string(key, output, ensure_ascii);
                output.push(':');
                if !compact {
                    output.push(' ');
                }
                write_value(item, output, ensure_ascii, compact);
            }
            output.push('}');
        }
    }
}

fn write_string(value: &str, output: &mut String, ensure_ascii: bool) {
    use std::fmt::Write as _;
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            character if (character as u32) < 0x20 => {
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character if ensure_ascii && (character as u32) > 0x7F => {
                let code = character as u32;
                if code > 0xFFFF {
                    let value = code - 0x10000;
                    let high = 0xD800 + (value >> 10);
                    let low = 0xDC00 + (value & 0x3FF);
                    let _ = write!(output, "\\u{high:04x}\\u{low:04x}");
                } else {
                    let _ = write!(output, "\\u{code:04x}");
                }
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

/// `repr(float)` — shortest round-trip digits, Python's notation thresholds.
pub fn format_float(value: f64) -> String {
    use std::fmt::Write as _;
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "Infinity".to_owned()
        } else {
            "-Infinity".to_owned()
        };
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_owned()
        } else {
            "0.0".to_owned()
        };
    }
    let negative = value.is_sign_negative();
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((scientific.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa
        .chars()
        .filter(|character| *character != '.')
        .collect();
    let mut output = String::new();
    if negative {
        output.push('-');
    }
    if !(-4..16).contains(&exponent) {
        output.push_str(&digits[..1]);
        if digits.len() > 1 {
            output.push('.');
            output.push_str(&digits[1..]);
        }
        output.push('e');
        if exponent < 0 {
            output.push('-');
        } else {
            output.push('+');
        }
        let magnitude = exponent.unsigned_abs();
        if magnitude < 10 {
            output.push('0');
        }
        let _ = write!(output, "{magnitude}");
    } else if exponent >= 0 {
        let integer_length = (exponent + 1) as usize;
        if digits.len() <= integer_length {
            output.push_str(&digits);
            for _ in 0..(integer_length - digits.len()) {
                output.push('0');
            }
            output.push_str(".0");
        } else {
            output.push_str(&digits[..integer_length]);
            output.push('.');
            output.push_str(&digits[integer_length..]);
        }
    } else {
        output.push_str("0.");
        for _ in 0..(-exponent - 1) {
            output.push('0');
        }
        output.push_str(&digits);
    }
    output
}
