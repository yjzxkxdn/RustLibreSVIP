//! Python-compatible JSON rendering (`json.dumps`).
//!
//! The Python CLI mutates `json._default_encoder` in `libresvip.core.compat`,
//! so compact dumps use `","`/`":"` separators while `json.dumps(..., indent=2)`
//! keeps the default `","`/`": "` separators. Strings are ASCII-escaped, which
//! is Python's `ensure_ascii=True` default.

use serde_json::{Number, Value};

/// `json.dumps(value)` with the patched default encoder (compact separators).
pub fn dumps(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, None, 0);
    out
}

/// `json.dumps(value, indent=2)`.
pub fn dumps_pretty(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, Some(2), 0);
    out
}

fn write_value(out: &mut String, value: &Value, indent: Option<usize>, depth: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => out.push_str(&format_number(number)),
        Value::String(text) => write_string(out, text),
        Value::Array(items) => write_array(out, items, indent, depth),
        Value::Object(map) => write_object(out, map, indent, depth),
    }
}

fn write_array(out: &mut String, items: &[Value], indent: Option<usize>, depth: usize) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        match indent {
            Some(width) => {
                out.push('\n');
                push_indent(out, width * (depth + 1));
            }
            None => {}
        }
        write_value(out, item, indent, depth + 1);
    }
    if let Some(width) = indent {
        out.push('\n');
        push_indent(out, width * depth);
    }
    out.push(']');
}

fn write_object(
    out: &mut String,
    map: &serde_json::Map<String, Value>,
    indent: Option<usize>,
    depth: usize,
) {
    if map.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push('{');
    for (index, (key, value)) in map.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        match indent {
            Some(width) => {
                out.push('\n');
                push_indent(out, width * (depth + 1));
            }
            None => {}
        }
        write_string(out, key);
        out.push(':');
        if indent.is_some() {
            out.push(' ');
        }
        write_value(out, value, indent, depth + 1);
    }
    if let Some(width) = indent {
        out.push('\n');
        push_indent(out, width * depth);
    }
    out.push('}');
}

fn push_indent(out: &mut String, count: usize) {
    for _ in 0..count {
        out.push(' ');
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            character if !(' '..='~').contains(&character) => {
                let code = character as u32;
                if code <= 0xffff {
                    out.push_str(&format!("\\u{code:04x}"));
                } else {
                    let code = code - 0x1_0000;
                    let high = 0xd800 + (code >> 10);
                    let low = 0xdc00 + (code & 0x3ff);
                    out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
                }
            }
            character => out.push(character),
        }
    }
    out.push('"');
}

fn format_number(number: &Number) -> String {
    if let Some(value) = number.as_i64() {
        return value.to_string();
    }
    if let Some(value) = number.as_u64() {
        return value.to_string();
    }
    match number.as_f64() {
        Some(value) => float_repr(value),
        None => number.to_string(),
    }
}

/// Renders a float the way Python's `repr` does (shortest round-trip form,
/// exponent always signed and at least two digits).
pub fn float_repr(value: f64) -> String {
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
    let rendered = Number::from_f64(value)
        .map(|number| number.to_string())
        .unwrap_or_else(|| value.to_string());
    normalise_exponent(&rendered)
}

fn normalise_exponent(rendered: &str) -> String {
    let Some((mantissa, exponent)) = rendered.split_once(['e', 'E']) else {
        return rendered.to_owned();
    };
    let (sign, digits) = match exponent.strip_prefix('-') {
        Some(rest) => ('-', rest),
        None => ('+', exponent.strip_prefix('+').unwrap_or(exponent)),
    };
    let digits = if digits.len() < 2 {
        format!("0{digits}")
    } else {
        digits.to_owned()
    };
    format!("{mantissa}e{sign}{digits}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compact_matches_python() {
        assert_eq!(
            dumps(&json!({"a": 1, "b": [1, 2], "c": {"d": null}})),
            r#"{"a":1,"b":[1,2],"c":{"d":null}}"#
        );
    }

    #[test]
    fn pretty_matches_python() {
        assert_eq!(
            dumps_pretty(&json!({"handle": "x", "tracks": ["a (1 notes)"], "warnings": []})),
            "{\n  \"handle\": \"x\",\n  \"tracks\": [\n    \"a (1 notes)\"\n  ],\n  \"warnings\": []\n}"
        );
    }

    #[test]
    fn strings_are_ascii_escaped() {
        assert_eq!(dumps(&json!("VX-\u{03b2}\n")), r#""VX-\u03b2\n""#);
        assert_eq!(dumps(&json!("\u{1f600}")), r#""\ud83d\ude00""#);
    }

    #[test]
    fn floats_keep_python_form() {
        assert_eq!(dumps(&json!(120.0)), "120.0");
        assert_eq!(dumps(&json!(1.5)), "1.5");
        assert_eq!(float_repr(1e-7), "1e-07");
        assert_eq!(float_repr(1e16), "1e+16");
    }
}
