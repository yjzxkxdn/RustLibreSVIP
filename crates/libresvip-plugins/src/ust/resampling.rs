//! Port of `libresvip.plugins.ust.resampling`.

use libresvip_core::core::compat::{find_index, find_last_index};
use libresvip_core::model::point::Point;

/// `resampled`: one point per `interval` ticks between the extreme x values,
/// sampling with `interpolate_method`.
pub fn resampled(
    data: &[Point],
    interval: i64,
    interpolate_method: impl Fn(Point, Point, i64) -> f64,
) -> Vec<Point> {
    let Some(left_point) = data.iter().min_by_key(|point| point.x) else {
        return Vec::new();
    };
    let Some(right_point) = data.iter().max_by_key(|point| point.x) else {
        return Vec::new();
    };
    if interval <= 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut current = left_point.x;
    while current <= right_point.x {
        let prev_index = find_last_index(data, |point| point.x <= current);
        let next_index = find_index(data, |point| point.x >= current);
        if prev_index >= 0 && next_index >= 0 {
            let value = interpolate_method(
                data[prev_index as usize],
                data[next_index as usize],
                current,
            );
            // `int(...)` truncates toward zero.
            result.push(Point::new(current, value as i64));
        }
        current += interval;
    }
    result
}

/// `dot_resampled`: keep the left point's value.
pub fn dot_resampled(data: &[Point], interval: i64) -> Vec<Point> {
    resampled(data, interval, |prev, _next, _current| prev.y as f64)
}
