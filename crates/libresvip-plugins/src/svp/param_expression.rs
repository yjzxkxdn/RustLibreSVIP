//! Port of `libresvip.plugins.svp.param_expression`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::point::Point;

use crate::svp::math_utils::{
    binary_find_last_index, position_to_ticks, py_round, Interpolation, RangeInterval,
};
use crate::svp::model::{SVNote, SVPitchControl};
use crate::svp::synthv_pitch::{
    NoteStruct, PitchControl, PitchControlLayerGenerator, PitchControlPoint, SynthVPitchSimulator,
};

#[derive(Clone, Debug)]
pub struct CurveGenerator {
    pub base_value: i64,
    pub interpolation: Interpolation,
    pub point_list: Vec<Point>,
    pos_indexes: Vec<(i64, usize)>,
    pub interval: Option<RangeInterval>,
}

impl CurveGenerator {
    pub fn new(
        points: impl IntoIterator<Item = Point>,
        interpolation: Interpolation,
        base_value: i64,
        interval: Option<RangeInterval>,
    ) -> Self {
        let mut point_list: Vec<Point> = Vec::new();
        let mut current_pos: i64 = -1;
        let mut current_sum: i64 = 0;
        let mut overlap_count: i64 = 0;
        for point in points {
            if point.x != current_pos && current_pos >= 0 {
                let average = current_sum as f64 / overlap_count as f64;
                point_list.push(Point::new(current_pos, py_round(average)));
                current_sum = 0;
                overlap_count = 0;
            }
            current_sum += point.y;
            overlap_count += 1;
            current_pos = point.x;
        }
        if current_pos != -1 {
            let average = current_sum as f64 / overlap_count as f64;
            point_list.push(Point::new(current_pos, py_round(average)));
        }
        let pos_indexes = point_list
            .iter()
            .enumerate()
            .map(|(index, point)| (point.x, index))
            .collect();
        Self {
            base_value,
            interpolation,
            point_list,
            pos_indexes,
            interval,
        }
    }

    pub fn value_at_ticks(&self, ticks: i64) -> f64 {
        if self.point_list.is_empty()
            || self
                .interval
                .as_ref()
                .map(|interval| !interval.includes(ticks))
                .unwrap_or(false)
        {
            return self.base_value as f64;
        }
        let Some(index) = binary_find_last_index(&self.pos_indexes, ticks) else {
            return self.point_list[0].y as f64;
        };
        if index == self.point_list.len() - 1 {
            return self.point_list[self.point_list.len() - 1].y as f64;
        }
        let start = self.point_list[index];
        let end = self.point_list[index + 1];
        self.interpolation.interpolate(
            ticks as f64,
            (start.x as f64, start.y as f64),
            (end.x as f64, end.y as f64),
        )
    }

    pub fn get_converted_curve(&self, step: i64) -> Vec<Point> {
        let mut result: Vec<Point> = Vec::new();
        if self.point_list.is_empty() {
            result.push(Point::start_point_with(self.base_value));
            result.push(Point::end_point_with(self.base_value));
            return result;
        }
        let mut prev_point = self.point_list[0];
        result.push(Point::start_point_with(prev_point.y));
        result.push(prev_point);
        for current_point in &self.point_list[1..] {
            if prev_point.y == self.base_value && current_point.y == self.base_value {
                result.push(Point::new(prev_point.x, self.base_value));
                result.push(Point::new(current_point.x, self.base_value));
            } else {
                let mut position = prev_point.x + step;
                while position < current_point.x {
                    let value = self.interpolation.interpolate(
                        position as f64,
                        (prev_point.x as f64, prev_point.y as f64),
                        (current_point.x as f64, current_point.y as f64),
                    );
                    result.push(Point::new(position, py_round(value)));
                    position += step;
                }
            }
            prev_point = *current_point;
        }
        result.push(Point::new(prev_point.x, prev_point.y));
        result.push(Point::end_point_with(prev_point.y));
        result
    }
}

/// `ParamExpression`: only addition of curve generators is used by this plugin.
#[derive(Clone, Debug)]
pub enum ParamExpr {
    Curve(CurveGenerator),
    Sum(Vec<ParamExpr>),
}

impl ParamExpr {
    pub fn value_at_ticks(&self, ticks: i64) -> f64 {
        match self {
            ParamExpr::Curve(curve) => curve.value_at_ticks(ticks),
            ParamExpr::Sum(expressions) => expressions
                .iter()
                .map(|expression| expression.value_at_ticks(ticks))
                .sum(),
        }
    }

    pub fn add(self, other: ParamExpr) -> ParamExpr {
        let mut expressions = match self {
            ParamExpr::Sum(expressions) => expressions,
            other => vec![other],
        };
        match other {
            ParamExpr::Sum(mut inner) => expressions.append(&mut inner),
            other => expressions.push(other),
        }
        ParamExpr::Sum(expressions)
    }
}

/// `PitchGenerator`.
#[derive(Clone, Debug)]
pub struct PitchGenerator {
    synchronizer: TimeSynchronizer,
    pitch_diff: ParamExpr,
    vibrato_env: ParamExpr,
    pitch_simulator: SynthVPitchSimulator,
    pub pitch_control_layer: Option<PitchControlLayerGenerator>,
}

impl PitchGenerator {
    pub fn new(
        synchronizer: TimeSynchronizer,
        note_list: &[SVNote],
        pitch_diff: ParamExpr,
        vibrato_env: ParamExpr,
        pitch_controls: Option<&[SVPitchControl]>,
    ) -> Self {
        let note_structs: Vec<NoteStruct> = note_list
            .iter()
            .map(|note| NoteStruct {
                key: note.pitch,
                start: synchronizer.get_actual_secs_from_ticks(position_to_ticks(note.onset)),
                end: synchronizer
                    .get_actual_secs_from_ticks(position_to_ticks(note.onset + note.duration)),
                portamento_offset: note.attributes.transition_offset(),
                portamento_left: note.attributes.portamento_left(),
                portamento_right: note.attributes.portamento_right(),
                depth_left: note.attributes.depth_left(),
                depth_right: note.attributes.depth_right(),
                vibrato_start: note.attributes.vibrato_start(),
                vibrato_left: note.attributes.vibrato_left(),
                vibrato_right: note.attributes.vibrato_right(),
                vibrato_depth: note.attributes.vibrato_depth(),
                vibrato_frequency: note.attributes.vibrato_frequency(),
                vibrato_phase: note.attributes.vibrato_phase(),
            })
            .collect();
        let pitch_controls: Option<Vec<PitchControl>> = pitch_controls.map(|controls| {
            controls
                .iter()
                .map(|control| PitchControl {
                    pos: position_to_ticks(control.pos()),
                    pitch: control.pitch(),
                    type_name: control.type_name().to_owned(),
                    points: control
                        .points()
                        .iter()
                        .map(|point| PitchControlPoint {
                            offset: position_to_ticks(point.offset),
                            value: point.value,
                        })
                        .collect(),
                })
                .collect()
        });
        let pitch_simulator = SynthVPitchSimulator::new(
            synchronizer.clone(),
            &note_structs,
            pitch_controls.as_deref(),
        );
        let pitch_control_layer = pitch_simulator.pitch_control_layer.clone();
        Self {
            synchronizer,
            pitch_diff,
            vibrato_env,
            pitch_simulator,
            pitch_control_layer,
        }
    }

    pub fn value_at_ticks(&self, ticks: i64) -> f64 {
        self.value_at_secs(self.synchronizer.get_actual_secs_from_ticks(ticks))
    }

    pub fn value_at_secs(&self, secs: f64) -> f64 {
        let ticks = py_round(self.synchronizer.get_actual_ticks_from_secs(secs));
        self.pitch_simulator.pitch_at_secs(secs, Some(ticks))
            + self.pitch_diff.value_at_ticks(ticks) * self.vibrato_env.value_at_ticks(ticks) / 1000.0
    }
}
