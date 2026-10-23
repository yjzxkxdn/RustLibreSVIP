//! Port of `libresvip.plugins.ust.rdp_simplification` (Ramer–Douglas–Peucker).

use libresvip_core::model::point::Point;

/// `perpendicular_distance`.
fn perpendicular_distance(point: Point, line_start: Point, line_end: Point) -> f64 {
    let mut dx = (line_end.x - line_start.x) as f64;
    let mut dy = (line_end.y - line_start.y) as f64;
    let magnitude = dx.hypot(dy);
    if magnitude > 0.0 {
        dx /= magnitude;
        dy /= magnitude;
    }
    let pvx = (point.x - line_start.x) as f64;
    let pvy = (point.y - line_start.y) as f64;
    let pvdot = dx * pvx + dy * pvy;
    let ax = pvx - pvdot * dx;
    let ay = pvy - pvdot * dy;
    ax.hypot(ay)
}

/// `simplify_shape`.
pub fn simplify_shape(point_list: &[Point], epsilon: f64) -> Vec<Point> {
    if point_list.len() < 2 {
        return point_list.to_vec();
    }
    let end = point_list.len() - 1;
    let mut dmax = 0.0f64;
    let mut index = 0usize;
    for (offset, point) in point_list.iter().enumerate().take(end).skip(1) {
        let distance = perpendicular_distance(*point, point_list[0], point_list[end]);
        if distance > dmax {
            index = offset;
            dmax = distance;
        }
    }
    if dmax <= epsilon {
        return vec![point_list[0], point_list[end]];
    }
    let first_line = &point_list[..index + 1];
    let last_line = &point_list[index..];
    let mut first_result = simplify_shape(first_line, epsilon);
    let last_result = simplify_shape(last_line, epsilon);
    first_result.pop();
    first_result.extend(last_result);
    first_result
}

/// `simplify_shape_to`: increase epsilon until fewer than `max_point_count`
/// points remain.
pub fn simplify_shape_to(point_list: &[Point], max_point_count: usize) -> Vec<Point> {
    let step = 0.05f64;
    let mut epsilon = step;
    loop {
        let result = simplify_shape(point_list, epsilon);
        if result.len() < max_point_count {
            return result;
        }
        epsilon += step;
    }
}
