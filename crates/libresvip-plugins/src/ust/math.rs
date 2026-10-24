//! Port of the `libresvip.utils.music_math` helpers used by the UST/USTX
//! plugins, plus Python-compatible number formatting helpers.

/// `clamp(x, lower, upper)`.
#[inline]
pub fn clamp(x: f64, lower: f64, upper: f64) -> f64 {
    x.max(lower).min(upper)
}

/// Python's `round(value, ndigits)` (ties to even on the exact binary value).
///
/// CPython rounds the *exact* value of the double to `ndigits` decimals with
/// round-half-to-even; Rust's fixed-precision formatting does the same, so the
/// result only needs to be parsed back to a float.
pub fn round_ndigits(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let text = format!("{value:.digits$}");
    text.parse::<f64>().unwrap_or(value)
}

/// `db_to_float(db, using_amplitude)`.
#[inline]
pub fn db_to_float(db: f64, using_amplitude: bool) -> f64 {
    if using_amplitude {
        10f64.powf(db / 20.0)
    } else {
        10f64.powf(db / 10.0)
    }
}

/// Python's `str(float)` / `repr(float)`.
///
/// Rust's `Display` never switches to exponent notation, whereas CPython uses
/// scientific notation for exponents below -4 or at least 16, so the shortest
/// round-trip digits are re-laid-out here.
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_owned()
        } else {
            "0.0".to_owned()
        };
    }
    let negative = value < 0.0;
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("LowerExp always contains an exponent");
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if negative { "-" } else { "" };
    if !(-4..16).contains(&exponent) {
        let mut out = String::new();
        out.push_str(sign);
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        if exponent < 0 {
            out.push('-');
        } else {
            out.push('+');
        }
        let magnitude = exponent.unsigned_abs();
        if magnitude < 10 {
            out.push('0');
        }
        out.push_str(&magnitude.to_string());
        return out;
    }
    // Positional notation.
    let point = exponent + 1;
    let mut out = String::new();
    out.push_str(sign);
    if point <= 0 {
        out.push_str("0.");
        for _ in 0..(-point) {
            out.push('0');
        }
        out.push_str(digits);
    } else if (point as usize) >= digits.len() {
        out.push_str(digits);
        for _ in 0..(point as usize - digits.len()) {
            out.push('0');
        }
        out.push_str(".0");
    } else {
        out.push_str(&digits[..point as usize]);
        out.push('.');
        out.push_str(&digits[point as usize..]);
    }
    out
}

// TODO(hub-v2): the interpolation machinery below moved to
// `libresvip_core::semantics::pitch::interpolation` in the T4 migration; these
// re-exports keep `ust` and `ustx` callers working unchanged. Not every name
// has a caller left, hence the allow.
#[allow(unused_imports)]
pub use libresvip_core::semantics::pitch::interpolation::{
    cubic_interpolation, cosine_easing_in, cosine_easing_in_out, cosine_easing_out,
    inner_interpolate, interpolate_cosine_ease_in, interpolate_cosine_ease_in_out,
    interpolate_cosine_ease_out, interpolate_linear, interpolate_shape, linear,
};

/// Python's `round(float)` — half to even — as an integer.
#[inline]
pub fn round_int(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// Python's floor division for integers (`//`).
#[inline]
pub fn div_floor(a: i64, b: i64) -> i64 {
    let quotient = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        quotient - 1
    } else {
        quotient
    }
}

/// Python truthiness for a float (NaN is truthy).
#[inline]
pub fn truthy(value: f64) -> bool {
    value != 0.0
}
