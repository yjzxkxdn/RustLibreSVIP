//! Port of the `libresvip.utils.music_math` helpers used by the UST/USTX
//! plugins, plus Python-compatible number formatting helpers.

use libresvip_core::model::point::Point;

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

// ---------------------------------------------------------------------------
// Interpolation between two points
// ---------------------------------------------------------------------------

/// `linear_interpolation`.
#[inline]
pub fn linear(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let r = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * r
}

#[inline]
fn ease_in(r: f64) -> f64 {
    1.0 - (r * std::f64::consts::PI / 2.0).cos()
}

#[inline]
fn ease_out(r: f64) -> f64 {
    (r * std::f64::consts::PI / 2.0).sin()
}

#[inline]
fn ease_in_out(r: f64) -> f64 {
    (1.0 - (r * std::f64::consts::PI).cos()) / 2.0
}

#[inline]
fn cubic(r: f64) -> f64 {
    (3.0 - 2.0 * r) * r * r
}

fn transform(x: f64, start: (f64, f64), end: (f64, f64), func: impl Fn(f64) -> f64) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let r = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * func(r)
}

/// `cosine_easing_in_interpolation`.
pub fn cosine_easing_in(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    transform(x, start, end, ease_in)
}

/// `cosine_easing_out_interpolation`.
pub fn cosine_easing_out(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    transform(x, start, end, ease_out)
}

/// `cosine_easing_in_out_interpolation`.
pub fn cosine_easing_in_out(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    transform(x, start, end, ease_in_out)
}

/// `cubic_interpolation`.
pub fn cubic_interpolation(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    transform(x, start, end, cubic)
}

/// `interpolate_shape` from `libresvip.plugins.ustx.utils.music_math`.
pub fn interpolate_shape(x: f64, start: (f64, f64), end: (f64, f64), shape: &str) -> f64 {
    match shape {
        "io" => cosine_easing_in_out(x, start, end),
        "i" => cosine_easing_in(x, start, end),
        "o" => cosine_easing_out(x, start, end),
        "sp" => cubic_interpolation(x, start, end),
        _ => linear(x, start, end),
    }
}

// ---------------------------------------------------------------------------
// Dense resampling (`_inner_interpolate` and friends)
// ---------------------------------------------------------------------------

/// Python's `round(float)` — half to even — as an integer.
#[inline]
pub fn round_int(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// `_inner_interpolate`: keeps the endpoints, inserts one point every
/// `interval` ticks in between.
pub fn inner_interpolate(
    data: &[Point],
    interval: i64,
    mapping: impl Fn(f64, (f64, f64), (f64, f64)) -> f64,
) -> Vec<Point> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut result: Vec<Point> = Vec::with_capacity(data.len());
    result.push(data[0]);
    for pair in data.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let start_f = (start.x as f64, start.y as f64);
        let end_f = (end.x as f64, end.y as f64);
        if interval > 0 {
            let mut x = start.x + 1;
            while x < end.x {
                result.push(Point::new(
                    x,
                    round_int(mapping(x as f64, start_f, end_f)),
                ));
                x += interval;
            }
        }
    }
    result.push(data[data.len() - 1]);
    result
}

/// `interpolate_linear`.
pub fn interpolate_linear(data: &[Point], interval: i64) -> Vec<Point> {
    inner_interpolate(data, interval, linear)
}

/// `interpolate_cosine_ease_in_out`.
pub fn interpolate_cosine_ease_in_out(data: &[Point], interval: i64) -> Vec<Point> {
    inner_interpolate(data, interval, cosine_easing_in_out)
}

/// `interpolate_cosine_ease_in`.
pub fn interpolate_cosine_ease_in(data: &[Point], interval: i64) -> Vec<Point> {
    inner_interpolate(data, interval, cosine_easing_in)
}

/// `interpolate_cosine_ease_out`.
pub fn interpolate_cosine_ease_out(data: &[Point], interval: i64) -> Vec<Point> {
    inner_interpolate(data, interval, cosine_easing_out)
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
