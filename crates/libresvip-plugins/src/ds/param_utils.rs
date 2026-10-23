//! Port of `libresvip.plugins.ds.utils.pitch_param_utils` and
//! `gender_param_utils`.

use libresvip_core::core::compat::{binary_find_first, binary_find_last};
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::ParamCurve;
use libresvip_core::model::point::Point;

use super::music_math::midi2hz;

/// `DsParamNode`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DsParamNode {
    pub time: f64,
    pub value: f64,
}

/// `DsParamCurve`.
#[derive(Clone, Debug, PartialEq)]
pub struct DsParamCurve {
    pub step_size: f64,
    pub point_list: Option<Vec<DsParamNode>>,
}

impl Default for DsParamCurve {
    fn default() -> Self {
        Self {
            step_size: 0.005,
            point_list: None,
        }
    }
}

/// `PitchParamUtils.value_at`.
fn pitch_value_at(segment: &[Point], ticks: f64) -> f64 {
    let left_point = segment.iter().rev().find(|point| point.x as f64 <= ticks);
    let Some(left_point) = left_point else {
        return segment[0].y as f64;
    };
    let right_point = segment.iter().find(|point| point.x as f64 > ticks);
    let Some(right_point) = right_point else {
        return segment[segment.len() - 1].y as f64;
    };
    let ratio = (ticks - left_point.x as f64) / (right_point.x - left_point.x) as f64;
    (1.0 - ratio) * left_point.y as f64 + ratio * right_point.y as f64
}

/// `GenderParamUtils.value_at`.
fn gender_value_at(segment: &[Point], ticks: f64) -> f64 {
    let Some(left_point) = binary_find_last(segment, |point| point.x as f64 <= ticks) else {
        return segment[0].y as f64;
    };
    let Some(right_point) = binary_find_first(segment, |point| point.x as f64 > ticks) else {
        return segment[segment.len() - 1].y as f64;
    };
    let ratio = (ticks - left_point.x as f64) / (right_point.x - left_point.x) as f64;
    (1.0 - ratio) * left_point.y as f64 + ratio * right_point.y as f64
}

/// `PitchParamUtils.encode`.
pub fn encode_pitch(curve: &ParamCurve, end: i64, time_step: f64) -> DsParamCurve {
    let end = end + 1920;
    let valid_points: Vec<Point> = curve
        .points
        .iter()
        .copied()
        .filter(|point| point.x >= 1930 && point.x + 10 < end && point.y >= 0)
        .collect();
    let point_list = if valid_points.is_empty() {
        None
    } else {
        let mut nodes = Vec::new();
        let mut position = 1920i64;
        while position < end {
            nodes.push(DsParamNode {
                time: position as f64 / 1000.0,
                value: midi2hz(pitch_value_at(&valid_points, position as f64) / 100.0),
            });
            position += 5;
        }
        Some(nodes)
    };
    DsParamCurve {
        step_size: time_step,
        point_list,
    }
}

/// `GenderParamUtils.encode`.
///
/// Python indexes `segment[0]` inside `value_at` when no gender point survives
/// the filter, so an affected range raises `IndexError`; an error is returned
/// here instead of panicking.
pub fn encode_gender(curve: &ParamCurve, end: i64, time_step: f64) -> Result<DsParamCurve> {
    let end = end + 1920;
    let valid_points: Vec<Point> = curve
        .points
        .iter()
        .copied()
        .filter(|point| point.x >= 1930 && point.x + 10 < end && (-1000..=1000).contains(&point.y))
        .collect();
    let mut nodes = Vec::new();
    let mut position = 1920i64;
    while position < end {
        if valid_points.is_empty() {
            return Err(LibreSvipError::msg("list index out of range"));
        }
        nodes.push(DsParamNode {
            time: position as f64 / 1000.0,
            value: gender_value_at(&valid_points, position as f64) / 1000.0,
        });
        position += 5;
    }
    Ok(DsParamCurve {
        step_size: time_step,
        point_list: Some(nodes),
    })
}
