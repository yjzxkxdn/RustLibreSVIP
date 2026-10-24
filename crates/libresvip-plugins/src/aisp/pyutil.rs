//! Small helpers reproducing CPython's `repr`/`round`/`json.dumps` behaviour.
//!
//! The AISingers files are two compact JSON documents written with
//! `json.dumps(..., separators=(", ", ": "))` and `ensure_ascii=True`, so the
//! exact spacing, escaping and float spelling matter byte for byte.

/// `round(value, ndigits)` for a finite float.
///
/// CPython rounds the exact binary value half-to-even and returns the double
/// nearest to that decimal. Rust's fixed-precision formatting does the same,
/// so formatting and re-parsing is the faithful equivalent (a
/// `value * 100` trick would introduce a second rounding error).
pub fn round_ndigits(value: f64, ndigits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{:.*}", ndigits, value)
        .parse::<f64>()
        .unwrap_or(value)
}

/// `repr(float)` (which is also `str(float)` since Python 3).
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".to_owned()
        } else {
            "Infinity".to_owned()
        };
    }
    let rendered = format!("{value:?}");
    match rendered.find(['e', 'E']) {
        Some(position) => {
            let (mantissa, exponent) = rendered.split_at(position);
            let exponent = &exponent[1..];
            let (sign, digits) = match exponent.strip_prefix('-') {
                Some(rest) => ('-', rest),
                None => ('+', exponent.strip_prefix('+').unwrap_or(exponent)),
            };
            format!("{mantissa}e{sign}{digits:0>2}")
        }
        None => rendered,
    }
}

/// Appends `value` as a JSON string literal, escaping exactly like
/// `json.dumps(..., ensure_ascii=True)`.
pub fn push_str(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character if (character as u32) <= 0x7f => out.push(character),
            character => {
                let code = character as u32;
                if code <= 0xFFFF {
                    out.push_str(&format!("\\u{code:04x}"));
                } else {
                    let value = code - 0x1_0000;
                    let high = 0xD800 + (value >> 10);
                    let low = 0xDC00 + (value & 0x3FF);
                    out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
                }
            }
        }
    }
    out.push('"');
}

pub fn push_opt_str(out: &mut String, value: Option<&str>) {
    match value {
        Some(value) => push_str(out, value),
        None => out.push_str("null"),
    }
}

pub fn push_i64(out: &mut String, value: i64) {
    out.push_str(&value.to_string());
}

pub fn push_opt_i64(out: &mut String, value: Option<i64>) {
    match value {
        Some(value) => push_i64(out, value),
        None => out.push_str("null"),
    }
}

pub fn push_f64(out: &mut String, value: f64) {
    out.push_str(&float_repr(value));
}

pub fn push_opt_f64(out: &mut String, value: Option<f64>) {
    match value {
        Some(value) => push_f64(out, value),
        None => out.push_str("null"),
    }
}

// Python's `round(float)` — ties to even, returns an integer.
pub use libresvip_core::core::compat::py_round;

/// Python's floor division for floats (`a // b`).
pub fn floordiv_f64(a: f64, b: f64) -> f64 {
    (a / b).floor()
}

/// Python's `%` for floats (the result has the sign of the divisor).
pub fn pymod_f64(a: f64, b: f64) -> f64 {
    a - (a / b).floor() * b
}

