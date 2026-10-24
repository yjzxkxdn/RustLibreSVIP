//! TODO(hub-v2): only the UTAU `PitchBendMode` mapping remains here; the
//! shape machinery lives in `libresvip_core::semantics::pitch::interpolation`
//! since the T4 migration.

use libresvip_core::model::point::Point;
use libresvip_core::semantics::pitch::interpolation::{interpolate as interpolate_with, CurveShape};

use super::model::PitchBendMode;

/// `interpolate`.
pub fn interpolate(
    last_point: Point,
    this_point: Point,
    curve_type: PitchBendMode,
    sampling_interval_tick: i64,
) -> Vec<Point> {
    let shape = match curve_type {
        PitchBendMode::S => CurveShape::Linear,
        PitchBendMode::J => CurveShape::EaseIn,
        PitchBendMode::R => CurveShape::EaseOut,
        PitchBendMode::Empty => CurveShape::EaseInOut,
    };
    interpolate_with(last_point, this_point, shape, sampling_interval_tick)
}
