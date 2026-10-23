//! Port of `libresvip.plugins.ust.interpolation`.

use libresvip_core::model::point::Point;

use super::math::{
    interpolate_cosine_ease_in, interpolate_cosine_ease_in_out, interpolate_cosine_ease_out,
    interpolate_linear,
};
use super::model::PitchBendMode;

/// `interpolate`.
pub fn interpolate(
    last_point: Point,
    this_point: Point,
    curve_type: PitchBendMode,
    sampling_interval_tick: i64,
) -> Vec<Point> {
    let input_data = [last_point, this_point];
    match curve_type {
        PitchBendMode::S => interpolate_linear(&input_data, sampling_interval_tick),
        PitchBendMode::J => interpolate_cosine_ease_in(&input_data, sampling_interval_tick),
        PitchBendMode::R => interpolate_cosine_ease_out(&input_data, sampling_interval_tick),
        PitchBendMode::Empty => {
            interpolate_cosine_ease_in_out(&input_data, sampling_interval_tick)
        }
    }
}
