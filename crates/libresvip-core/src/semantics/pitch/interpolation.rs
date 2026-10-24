//! Interpolation between curve points — port of the shape helpers in
//! `libresvip.plugins.ust.music_math`, lifted to the format-agnostic layer.

use crate::core::compat::py_round;
use crate::model::point::Point;

/// Curve shape between two anchor points.
///
/// UTAU's `PitchBendMode` (S/J/R/Empty) and USTX's shape strings
/// (`"lo"`/`"i"`/`"o"`/`"io"`/`"sp"`) both map onto this enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurveShape {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Cubic,
}

impl CurveShape {
    /// USTX shape string (`"lo"`, `"i"`, `"o"`, `"io"`, `"sp"`); unknown
    /// strings are linear, like `interpolate_shape`.
    pub fn from_ustx_shape(shape: &str) -> Self {
        match shape {
            "i" => CurveShape::EaseIn,
            "o" => CurveShape::EaseOut,
            "io" => CurveShape::EaseInOut,
            "sp" => CurveShape::Cubic,
            _ => CurveShape::Linear,
        }
    }
}

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

fn shape_mapping(shape: CurveShape) -> fn(f64, (f64, f64), (f64, f64)) -> f64 {
    match shape {
        CurveShape::Linear => linear,
        CurveShape::EaseIn => cosine_easing_in,
        CurveShape::EaseOut => cosine_easing_out,
        CurveShape::EaseInOut => cosine_easing_in_out,
        CurveShape::Cubic => cubic_interpolation,
    }
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
                    py_round(mapping(x as f64, start_f, end_f)),
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

/// Dense sampling between two anchor points with the given shape — the
/// semantics counterpart of UTAU's `interpolate`.
pub fn interpolate(
    last_point: Point,
    this_point: Point,
    shape: CurveShape,
    sampling_interval_tick: i64,
) -> Vec<Point> {
    let input_data = [last_point, this_point];
    inner_interpolate(&input_data, sampling_interval_tick, shape_mapping(shape))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_strings_map_like_interpolate_shape() {
        assert_eq!(CurveShape::from_ustx_shape("io"), CurveShape::EaseInOut);
        assert_eq!(CurveShape::from_ustx_shape("sp"), CurveShape::Cubic);
        assert_eq!(CurveShape::from_ustx_shape("lo"), CurveShape::Linear);
        assert_eq!(CurveShape::from_ustx_shape("??"), CurveShape::Linear);
    }

    #[test]
    fn dense_interpolation_keeps_endpoints() {
        let points = interpolate(
            Point::new(0, 0),
            Point::new(10, 100),
            CurveShape::Linear,
            5,
        );
        assert_eq!(points.first().copied(), Some(Point::new(0, 0)));
        assert_eq!(points.last().copied(), Some(Point::new(10, 100)));
        assert_eq!(points.len(), 4); // 0, 1, 6 and the endpoint
        assert_eq!(points[1], Point::new(1, 10));
        assert_eq!(points[2], Point::new(6, 60));
    }
}
