//! Pydantic-compatible option parsing for the MCP tools.
//!
//! The Python server validates the JSON option string with
//! `option_cls.model_validate_json(raw)` and reports `ValidationError.json()`
//! as the tool error message. Rust has no pydantic, so this module reproduces
//! the observable behaviour:
//!
//! * unknown keys are ignored (`BaseModel` does not forbid extra fields),
//! * lax scalar coercion (`"2"` -> 2, `true` -> 1.0, `"yes"` -> true, …),
//! * enum options accept only their member values,
//! * validation failures render the same `[{type, loc, msg, input, ctx, url}]`
//!   payload pydantic emits, including the documented per-error-class URL.
//!
//! The only intentionally hard-coded pydantic detail is the documentation URL
//! version (`PYDANTIC_VERSION`), which `scripts/check_mcp.py` normalises.

use libresvip_core::options::{OptionFieldDef, OptionKind, OptionValue, Options};
use serde_json::{json, Value};

/// The pydantic release the Python reference reports in error URLs.
pub const PYDANTIC_VERSION: &str = "2.14";

pub fn error_url(kind: &str) -> String {
    format!("https://errors.pydantic.dev/{PYDANTIC_VERSION}/v/{kind}")
}

/// Validates `raw` (a JSON object string, or `None`) against an option schema.
///
/// `class_name` is the pydantic model class name reported by `model_type`
/// errors (`InputOptions`, `OutputOptions` or `ProcessOptions`).
pub fn parse(
    schema: &[OptionFieldDef],
    raw: Option<&str>,
    class_name: &str,
) -> Result<Options, String> {
    let Some(raw) = raw else {
        return Ok(Options::new());
    };
    if raw.is_empty() {
        return Ok(Options::new());
    }
    let parsed: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(error) => {
            let message = error.to_string();
            return Err(render_errors(vec![error_entry(
                "json_invalid",
                &[],
                format!("Invalid JSON: {message}"),
                &Value::String(raw.to_owned()),
                Some(json!({ "error": message })),
            )]));
        }
    };
    let Value::Object(values) = parsed else {
        return Err(render_errors(vec![error_entry(
            "model_type",
            &[],
            "Input should be an object",
            &parsed,
            Some(json!({ "class_name": class_name })),
        )]));
    };

    let mut options = Options::new();
    let mut errors = Vec::new();
    for field in schema {
        let Some(value) = values.get(field.key) else {
            continue;
        };
        match coerce(field, value) {
            Ok(converted) => {
                options.insert(field.key, converted);
            }
            Err((kind, message, ctx)) => {
                errors.push(error_entry(kind, &[field.key], message, value, ctx));
            }
        }
    }
    if !errors.is_empty() {
        return Err(render_errors(errors));
    }
    Ok(options)
}

type CoerceError = (&'static str, String, Option<Value>);

fn coerce(field: &OptionFieldDef, value: &Value) -> Result<OptionValue, CoerceError> {
    match &field.kind {
        OptionKind::Bool => coerce_bool(value),
        OptionKind::Int => coerce_int(value),
        OptionKind::Float => coerce_float(value),
        OptionKind::Str => coerce_string(value),
        OptionKind::Enum { variants, .. } => coerce_enum(value, variants),
        OptionKind::Complex { type_name, .. } | OptionKind::Opaque { type_name } => {
            coerce_complex(value, type_name)
        }
    }
}

fn coerce_bool(value: &Value) -> Result<OptionValue, CoerceError> {
    match value {
        Value::Bool(flag) => Ok(OptionValue::Bool(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(0) => Ok(OptionValue::Bool(false)),
            Some(1) => Ok(OptionValue::Bool(true)),
            _ => Err(bool_parsing()),
        },
        Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "0" | "off" | "f" | "false" | "n" | "no" => Ok(OptionValue::Bool(false)),
            "1" | "on" | "t" | "true" | "y" | "yes" => Ok(OptionValue::Bool(true)),
            _ => Err(bool_parsing()),
        },
        Value::Null => Err(bool_type()),
        _ => Err(bool_type()),
    }
}

fn bool_type() -> CoerceError {
    (
        "bool_type",
        "Input should be a valid boolean".to_owned(),
        None,
    )
}

fn bool_parsing() -> CoerceError {
    (
        "bool_parsing",
        "Input should be a valid boolean, unable to interpret input".to_owned(),
        None,
    )
}

fn coerce_int(value: &Value) -> Result<OptionValue, CoerceError> {
    match value {
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                return Ok(OptionValue::Int(integer));
            }
            if let Some(integer) = number.as_u64() {
                return i64::try_from(integer)
                    .map(OptionValue::Int)
                    .map_err(|_| int_type());
            }
            match number.as_f64() {
                Some(float) if float.fract() == 0.0 && float.is_finite() => {
                    Ok(OptionValue::Int(float as i64))
                }
                Some(_) => Err((
                    "int_from_float",
                    "Input should be a valid integer, got a number with a fractional part"
                        .to_owned(),
                    None,
                )),
                None => Err(int_type()),
            }
        }
        Value::String(text) => text
            .trim()
            .parse::<i64>()
            .map(OptionValue::Int)
            .map_err(|_| {
                (
                    "int_parsing",
                    "Input should be a valid integer, unable to parse string as an integer"
                        .to_owned(),
                    None,
                )
            }),
        Value::Bool(_) => Err(int_type()),
        Value::Null => Err(int_type()),
        _ => Err(int_type()),
    }
}

fn int_type() -> CoerceError {
    (
        "int_type",
        "Input should be a valid integer".to_owned(),
        None,
    )
}

fn coerce_float(value: &Value) -> Result<OptionValue, CoerceError> {
    match value {
        Value::Number(number) => number
            .as_f64()
            .map(OptionValue::Float)
            .ok_or_else(float_type),
        Value::Bool(flag) => Ok(OptionValue::Float(if *flag { 1.0 } else { 0.0 })),
        Value::String(text) => text.trim().parse::<f64>().map(OptionValue::Float).map_err(
            |_| {
                (
                    "float_parsing",
                    "Input should be a valid number, unable to parse string as a number"
                        .to_owned(),
                    None,
                )
            },
        ),
        Value::Null => Err(float_type()),
        _ => Err(float_type()),
    }
}

fn float_type() -> CoerceError {
    (
        "float_type",
        "Input should be a valid number".to_owned(),
        None,
    )
}

fn coerce_string(value: &Value) -> Result<OptionValue, CoerceError> {
    match value {
        Value::String(text) => Ok(OptionValue::Str(text.clone())),
        _ => Err((
            "string_type",
            "Input should be a valid string".to_owned(),
            None,
        )),
    }
}

fn coerce_enum(
    value: &Value,
    variants: &[libresvip_core::options::EnumVariant],
) -> Result<OptionValue, CoerceError> {
    if let Value::String(text) = value {
        if let Some(variant) = variants.iter().find(|variant| variant.value == text) {
            return Ok(OptionValue::Str(variant.value.to_owned()));
        }
    }
    let expected = expected_values(variants);
    Err((
        "enum",
        format!("Input should be {expected}"),
        Some(json!({ "expected": expected })),
    ))
}

/// `'a', 'b' or 'c'` — pydantic's `expected` wording.
fn expected_values(variants: &[libresvip_core::options::EnumVariant]) -> String {
    let quoted: Vec<String> = variants
        .iter()
        .map(|variant| format!("'{}'", variant.value))
        .collect();
    match quoted.len() {
        0 => String::new(),
        1 => quoted[0].clone(),
        len => format!("{} or {}", quoted[..len - 1].join(", "), quoted[len - 1]),
    }
}

fn coerce_complex(value: &Value, type_name: &str) -> Result<OptionValue, CoerceError> {
    // Rust plugins receive complex option fields as the textual form produced
    // by `BaseComplexModel.__str__`; JSON strings are passed through, scalar
    // values use their Python `str()` rendering and objects are serialised.
    match value {
        Value::String(text) => Ok(OptionValue::Str(text.clone())),
        Value::Bool(flag) => Ok(OptionValue::Str(if *flag {
            "True".to_owned()
        } else {
            "False".to_owned()
        })),
        Value::Number(number) => Ok(OptionValue::Str(number.to_string())),
        Value::Array(_) | Value::Object(_) => Ok(OptionValue::Str(crate::mcp::pyjson::dumps(value))),
        Value::Null => Err((
            "model_type",
            format!("Input should be a valid dictionary or instance of {type_name}"),
            Some(json!({ "class_name": type_name })),
        )),
    }
}

fn error_entry(
    kind: &str,
    loc: &[&str],
    msg: impl Into<String>,
    input: &Value,
    ctx: Option<Value>,
) -> Value {
    let mut entry = serde_json::Map::new();
    entry.insert("type".to_owned(), Value::String(kind.to_owned()));
    entry.insert(
        "loc".to_owned(),
        Value::Array(
            loc.iter()
                .map(|part| Value::String((*part).to_owned()))
                .collect(),
        ),
    );
    entry.insert("msg".to_owned(), Value::String(msg.into()));
    entry.insert("input".to_owned(), input.clone());
    if let Some(ctx) = ctx {
        entry.insert("ctx".to_owned(), ctx);
    }
    entry.insert("url".to_owned(), Value::String(error_url(kind)));
    Value::Object(entry)
}

/// `ValidationError.json()` — compact JSON array of error objects.
pub fn render_errors(errors: Vec<Value>) -> String {
    crate::mcp::pyjson::dumps(&Value::Array(errors))
}
