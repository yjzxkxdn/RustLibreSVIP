//! DiffSinger parameters file model — port of
//! `libresvip.plugins.ds.utils.models.ds_file`.
//!
//! Python validates every sequence field from its space-separated string form
//! (`[float(x) for x in value.split()]`) and serialises it back with
//! `" ".join(str(x) for x in value)`. The Rust model keeps the parsed form and
//! rebuilds the strings on output.

use serde_json::{Map, Number, Value};

use libresvip_core::error::{LibreSvipError, Result};

use super::music_math::float_repr;

/// One element of the DiffSinger parameters file (`DsItem`).
#[derive(Clone, Debug, Default)]
pub struct DsItem {
    pub text: Vec<String>,
    pub ph_seq: Vec<String>,
    pub note_seq: Vec<String>,
    pub note_dur: Option<Vec<f64>>,
    pub note_dur_seq: Option<Vec<f64>>,
    pub note_slur: Option<Vec<i64>>,
    pub note_glide: Option<Vec<String>>,
    pub is_slur_seq: Option<Vec<i64>>,
    pub ph_dur: Option<Vec<f64>>,
    pub ph_num: Option<Vec<i64>>,
    pub f0_timestep: Option<f64>,
    pub f0_seq: Option<Vec<f64>>,
    pub input_type: Option<String>,
    pub offset: f64,
    pub seed: Option<i64>,
    pub spk_mix: Option<Vec<(String, Vec<f64>)>>,
    pub spk_mix_timestep: Option<f64>,
    pub gender: Option<Vec<f64>>,
    pub gender_timestep: Option<f64>,
    pub velocity: Option<Vec<f64>>,
    pub velocity_timestep: Option<f64>,
}

fn type_error(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Parse(message.into())
}

fn string_list(value: Option<&Value>) -> Result<Option<Vec<String>>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(
            text.split_whitespace().map(str::to_owned).collect(),
        )),
        // Pydantic's `mode="before"` validators reject arrays, but accepting
        // them is harmless for files written by other tools.
        Some(Value::Array(items)) => {
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::String(text) => result.push(text.clone()),
                    other => result.push(other.to_string()),
                }
            }
            Ok(Some(result))
        }
        Some(_) => Err(type_error("expected a space separated string list")),
    }
}

fn float_list(value: Option<&Value>) -> Result<Option<Vec<f64>>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let mut result = Vec::new();
            for part in text.split_whitespace() {
                result.push(
                    part.parse::<f64>()
                        .map_err(|_| type_error(format!("could not convert string to float: '{part}'")))?,
                );
            }
            Ok(Some(result))
        }
        Some(Value::Array(items)) => {
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                result.push(match item {
                    Value::Number(number) => number.as_f64().unwrap_or(0.0),
                    Value::String(text) => text
                        .parse::<f64>()
                        .map_err(|_| type_error(format!("could not convert string to float: '{text}'")))?,
                    _ => return Err(type_error("expected a float")),
                });
            }
            Ok(Some(result))
        }
        Some(_) => Err(type_error("expected a space separated float list")),
    }
}

fn int_list(value: Option<&Value>) -> Result<Option<Vec<i64>>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let mut result = Vec::new();
            for part in text.split_whitespace() {
                result.push(
                    part.parse::<i64>()
                        .map_err(|_| type_error(format!("invalid literal for int(): '{part}'")))?,
                );
            }
            Ok(Some(result))
        }
        Some(Value::Array(items)) => {
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                result.push(match item {
                    Value::Number(number) => number.as_i64().unwrap_or(0),
                    Value::String(text) => text
                        .parse::<i64>()
                        .map_err(|_| type_error(format!("invalid literal for int(): '{text}'")))?,
                    _ => return Err(type_error("expected an int")),
                });
            }
            Ok(Some(result))
        }
        Some(_) => Err(type_error("expected a space separated int list")),
    }
}

fn number(value: Option<&Value>) -> Result<Option<f64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => Ok(number.as_f64()),
        Some(Value::String(text)) => text
            .parse::<f64>()
            .map(Some)
            .map_err(|_| type_error(format!("could not convert string to float: '{text}'"))),
        Some(_) => Err(type_error("expected a number")),
    }
}

fn mixed_value_lists(value: Option<&Value>) -> Result<Option<Vec<(String, Vec<f64>)>>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(map)) => {
            let mut result = Vec::with_capacity(map.len());
            for (key, item) in map {
                let values = float_list(Some(item))?.unwrap_or_default();
                result.push((key.clone(), values));
            }
            Ok(Some(result))
        }
        Some(_) => Err(type_error("expected an object of float lists")),
    }
}

impl DsItem {
    /// `DsProject.model_validate_json` for one item.
    pub fn from_value(value: &Value) -> Result<Self> {
        let Value::Object(map) = value else {
            return Err(type_error("DsItem must be a JSON object"));
        };
        let required = |name: &str| -> Result<Vec<String>> {
            let value = map
                .get(name)
                .ok_or_else(|| LibreSvipError::Parse(format!("missing field `{name}`")))?;
            string_list(Some(value))?
                .ok_or_else(|| LibreSvipError::Parse(format!("field `{name}` may not be null")))
        };
        let text = required("text")?;
        let ph_seq = required("ph_seq")?;
        let note_seq = required("note_seq")?;
        let offset = number(map.get("offset"))?.unwrap_or(0.0);
        Ok(Self {
            text,
            ph_seq,
            note_seq,
            note_dur: float_list(map.get("note_dur"))?,
            note_dur_seq: float_list(map.get("note_dur_seq"))?,
            note_slur: int_list(map.get("note_slur"))?,
            note_glide: string_list(map.get("note_glide"))?,
            is_slur_seq: int_list(map.get("is_slur_seq"))?,
            ph_dur: float_list(map.get("ph_dur"))?,
            ph_num: int_list(map.get("ph_num"))?,
            f0_timestep: number(map.get("f0_timestep"))?,
            f0_seq: float_list(map.get("f0_seq"))?,
            input_type: map
                .get("input_type")
                .and_then(Value::as_str)
                .map(str::to_owned),
            offset,
            seed: map.get("seed").and_then(Value::as_i64),
            spk_mix: mixed_value_lists(map.get("spk_mix"))?,
            spk_mix_timestep: number(map.get("spk_mix_timestep"))?,
            gender: float_list(map.get("gender"))?,
            gender_timestep: number(map.get("gender_timestep"))?,
            velocity: float_list(map.get("velocity"))?,
            velocity_timestep: number(map.get("velocity_timestep"))?,
        })
    }

    /// `DsItem.model_dump(mode="json")`.
    fn to_value(&self) -> Value {
        fn joined_strings(items: &[String]) -> String {
            items.join(" ")
        }
        fn joined_floats(items: &[f64]) -> String {
            items
                .iter()
                .map(|value| float_repr(*value))
                .collect::<Vec<_>>()
                .join(" ")
        }
        fn joined_ints(items: &[i64]) -> String {
            items
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        }
        fn float_number(value: f64) -> Value {
            Number::from_f64(value).map_or(Value::Null, Value::Number)
        }
        let mut map = Map::new();
        map.insert("text".to_owned(), Value::String(joined_strings(&self.text)));
        map.insert("ph_seq".to_owned(), Value::String(joined_strings(&self.ph_seq)));
        map.insert(
            "note_seq".to_owned(),
            Value::String(joined_strings(&self.note_seq)),
        );
        map.insert(
            "note_dur".to_owned(),
            self.note_dur
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_floats(items))),
        );
        map.insert(
            "note_dur_seq".to_owned(),
            self.note_dur_seq
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_floats(items))),
        );
        map.insert(
            "note_slur".to_owned(),
            self.note_slur
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_ints(items))),
        );
        map.insert(
            "note_glide".to_owned(),
            self.note_glide
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_strings(items))),
        );
        map.insert(
            "is_slur_seq".to_owned(),
            self.is_slur_seq
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_ints(items))),
        );
        map.insert(
            "ph_dur".to_owned(),
            self.ph_dur
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_floats(items))),
        );
        map.insert(
            "ph_num".to_owned(),
            self.ph_num
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_ints(items))),
        );
        map.insert(
            "f0_timestep".to_owned(),
            self.f0_timestep.map_or(Value::Null, float_number),
        );
        map.insert(
            "f0_seq".to_owned(),
            self.f0_seq
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_floats(items))),
        );
        map.insert(
            "input_type".to_owned(),
            self.input_type
                .as_ref()
                .map_or(Value::Null, |value| Value::String(value.clone())),
        );
        map.insert("offset".to_owned(), float_number(self.offset));
        map.insert(
            "seed".to_owned(),
            self.seed
                .map_or(Value::Null, |value| Value::Number(Number::from(value))),
        );
        map.insert(
            "spk_mix".to_owned(),
            self.spk_mix.as_ref().map_or(Value::Null, |entries| {
                let mut object = Map::new();
                for (key, values) in entries {
                    object.insert(key.clone(), Value::String(joined_floats(values)));
                }
                Value::Object(object)
            }),
        );
        map.insert(
            "spk_mix_timestep".to_owned(),
            self.spk_mix_timestep.map_or(Value::Null, float_number),
        );
        map.insert(
            "gender".to_owned(),
            self.gender
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_floats(items))),
        );
        map.insert(
            "gender_timestep".to_owned(),
            self.gender_timestep.map_or(Value::Null, float_number),
        );
        map.insert(
            "velocity".to_owned(),
            self.velocity
                .as_deref()
                .map_or(Value::Null, |items| Value::String(joined_floats(items))),
        );
        map.insert(
            "velocity_timestep".to_owned(),
            self.velocity_timestep.map_or(Value::Null, float_number),
        );
        Value::Object(map)
    }
}

/// Parses a whole `.ds` document (`DsProject(RootModel[list[DsItem]])`).
pub fn parse_project(text: &str) -> Result<Vec<DsItem>> {
    let value: Value = serde_json::from_str(text)?;
    let Value::Array(items) = value else {
        return Err(type_error("DsProject must be a JSON array"));
    };
    items.iter().map(DsItem::from_value).collect()
}

/// `json.dumps(ds_project.model_dump(mode="json"), indent=indent,
/// ensure_ascii=False)`.
pub fn dump_project(items: &[DsItem], indent: Option<i64>) -> String {
    let value = Value::Array(items.iter().map(DsItem::to_value).collect());
    dumps_py(&value, indent)
}

fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

fn number_string(number: &Number) -> String {
    if let Some(value) = number.as_i64() {
        value.to_string()
    } else if let Some(value) = number.as_u64() {
        value.to_string()
    } else {
        float_repr(number.as_f64().unwrap_or(0.0))
    }
}

fn write_value(value: &Value, indent: Option<i64>, level: usize, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => out.push_str(&number_string(number)),
        Value::String(text) => out.push_str(&escape_json(text)),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            match indent {
                None => {
                    out.push('[');
                    for (index, item) in items.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        write_value(item, indent, level + 1, out);
                    }
                    out.push(']');
                }
                Some(width) => {
                    let width = width.max(0) as usize;
                    out.push('[');
                    for (index, item) in items.iter().enumerate() {
                        out.push('\n');
                        for _ in 0..(width * (level + 1)) {
                            out.push(' ');
                        }
                        write_value(item, indent, level + 1, out);
                        if index + 1 < items.len() {
                            out.push(',');
                        }
                    }
                    out.push('\n');
                    for _ in 0..(width * level) {
                        out.push(' ');
                    }
                    out.push(']');
                }
            }
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            match indent {
                None => {
                    out.push('{');
                    for (index, (key, item)) in map.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(&escape_json(key));
                        out.push_str(": ");
                        write_value(item, indent, level + 1, out);
                    }
                    out.push('}');
                }
                Some(width) => {
                    let width = width.max(0) as usize;
                    out.push('{');
                    for (index, (key, item)) in map.iter().enumerate() {
                        out.push('\n');
                        for _ in 0..(width * (level + 1)) {
                            out.push(' ');
                        }
                        out.push_str(&escape_json(key));
                        out.push_str(": ");
                        write_value(item, indent, level + 1, out);
                        if index + 1 < map.len() {
                            out.push(',');
                        }
                    }
                    out.push('\n');
                    for _ in 0..(width * level) {
                        out.push(' ');
                    }
                    out.push('}');
                }
            }
        }
    }
}

/// Python's `json.dumps(value, indent=indent, ensure_ascii=False)`.
pub fn dumps_py(value: &Value, indent: Option<i64>) -> String {
    let mut out = String::new();
    write_value(value, indent, 0, &mut out);
    out
}
