//! Port of `libresvip.plugins.dspx.vibrato_utils`.

use std::collections::BTreeMap;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{ParamCurve, VibratoParam};
use libresvip_core::model::point::{Point, END_POINT_X, START_POINT_X};

use super::model::{ControlPoint, Vibrato, VibratoPoints};
use super::pyutil::{clamp, py_round};

pub const VIBRATO_POSITION_SCALE: f64 = 100000.0;
pub const VIBRATO_FREQUENCY_SCALE: f64 = 100.0;

#[derive(Clone, Debug, Default)]
pub struct ScaleCurve {
    points: Vec<ControlPoint>,
    positions: Vec<f64>,
}

impl ScaleCurve {
    pub fn new(points: &[ControlPoint]) -> Self {
        let mut sorted: Vec<(usize, ControlPoint)> =
            points.iter().copied().enumerate().collect();
        sorted.sort_by(|left, right| {
            left.1
                .x
                .partial_cmp(&right.1.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.0.cmp(&right.0))
        });
        let mut unique: BTreeMap<u64, ControlPoint> = BTreeMap::new();
        for (_, point) in sorted {
            unique.insert(point.x.to_bits(), point);
        }
        let mut unique_points: Vec<ControlPoint> = unique.into_values().collect();
        unique_points.sort_by(|left, right| {
            left.x
                .partial_cmp(&right.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let positions = unique_points.iter().map(|point| point.x).collect();
        Self {
            points: unique_points,
            positions,
        }
    }

    pub fn value_at(&self, position: f64) -> f64 {
        if self.points.is_empty() {
            return 1.0;
        }
        if position <= self.points[0].x {
            return self.points[0].y;
        }
        if position >= self.points[self.points.len() - 1].x {
            return self.points[self.points.len() - 1].y;
        }
        let right_index = self.positions.partition_point(|x| *x <= position);
        let left = self.points[right_index - 1];
        let right = self.points[right_index];
        let ratio = (position - left.x) / (right.x - left.x);
        left.y + (right.y - left.y) * ratio
    }

    pub fn integral_from_zero(&self, position: f64) -> f64 {
        if self.points.is_empty() {
            return position;
        }
        if position >= 0.0 {
            return self.integrate_forward(0.0, position);
        }
        -self.integrate_forward(position, 0.0)
    }

    fn integrate_forward(&self, start: f64, end: f64) -> f64 {
        if start >= end {
            return 0.0;
        }
        let mut result = 0.0;
        let mut cursor = start;
        while cursor < end {
            let right_index = self.positions.partition_point(|x| *x <= cursor);
            let next_position = if right_index < self.positions.len() {
                end.min(self.positions[right_index])
            } else {
                end
            };
            if cursor >= next_position {
                cursor = next_position;
                continue;
            }
            result += (self.value_at(cursor) + self.value_at(next_position)) * 0.5
                * (next_position - cursor);
            cursor = next_position;
        }
        result
    }
}

#[derive(Clone, Debug)]
pub struct VibratoCurve {
    vibrato: Vibrato,
    note_length_secs: f64,
    amplitude: ScaleCurve,
    frequency: ScaleCurve,
}

impl VibratoCurve {
    pub fn new(vibrato: &Vibrato, note_length_secs: f64) -> Self {
        Self {
            vibrato: vibrato.clone(),
            note_length_secs,
            amplitude: ScaleCurve::new(&vibrato.points.amp),
            frequency: ScaleCurve::new(&vibrato.points.freq),
        }
    }

    pub fn evaluate(&self, position: f64) -> f64 {
        if position < self.vibrato.start || position > self.vibrato.end {
            return 0.0;
        }
        let cycles = self.vibrato.freq
            * self.note_length_secs
            * self.frequency.integral_from_zero(position);
        let phase = 2.0 * std::f64::consts::PI * (cycles + self.vibrato.phase);
        self.vibrato.amp as f64 * self.amplitude.value_at(position) * phase.sin()
            + self.vibrato.offset as f64
    }
}

#[derive(Clone, Debug)]
struct NoteVibrato {
    start_tick: i64,
    end_tick: i64,
    start_secs: f64,
    duration_secs: f64,
    curve: VibratoCurve,
}

pub struct VibratoSequence {
    synchronizer: TimeSynchronizer,
    vibratos: Vec<NoteVibrato>,
    start_ticks: Vec<i64>,
    active_vibratos: Vec<usize>,
    next_vibrato_index: usize,
    last_tick: Option<i64>,
}

impl VibratoSequence {
    pub fn new(
        notes: &[super::model::Note],
        clip_start: i64,
        synchronizer: &TimeSynchronizer,
    ) -> Self {
        let mut vibratos = Vec::new();
        for note in notes {
            if note.length <= 0 {
                continue;
            }
            let start_tick = clip_start + note.pos;
            let end_tick = start_tick + note.length;
            let start_secs = synchronizer.get_actual_secs_from_ticks(start_tick);
            let duration_secs = synchronizer.get_duration_secs_from_ticks(start_tick, end_tick);
            if duration_secs <= 0.0 {
                continue;
            }
            vibratos.push(NoteVibrato {
                start_tick,
                end_tick,
                start_secs,
                duration_secs,
                curve: VibratoCurve::new(&note.vibrato, duration_secs),
            });
        }
        vibratos.sort_by_key(|vibrato| vibrato.start_tick);
        let start_ticks = vibratos.iter().map(|vibrato| vibrato.start_tick).collect();
        Self {
            synchronizer: synchronizer.clone(),
            vibratos,
            start_ticks,
            active_vibratos: Vec::new(),
            next_vibrato_index: 0,
            last_tick: None,
        }
    }

    pub fn evaluate(&mut self, relative_tick: i64, clip_start: i64) -> f64 {
        let absolute_tick = relative_tick + clip_start;
        match self.last_tick {
            Some(last_tick) if absolute_tick >= last_tick => {
                self.active_vibratos.retain(|index| {
                    self.vibratos[*index].end_tick > absolute_tick
                });
                while self.next_vibrato_index < self.vibratos.len()
                    && self.vibratos[self.next_vibrato_index].start_tick <= absolute_tick
                {
                    self.active_vibratos.push(self.next_vibrato_index);
                    self.next_vibrato_index += 1;
                }
            }
            _ => {
                self.next_vibrato_index = self
                    .start_ticks
                    .partition_point(|start_tick| *start_tick <= absolute_tick);
                self.active_vibratos = (0..self.next_vibrato_index)
                    .filter(|index| self.vibratos[*index].end_tick > absolute_tick)
                    .collect();
            }
        }
        self.last_tick = Some(absolute_tick);
        if self.active_vibratos.is_empty() {
            return 0.0;
        }
        let seconds = self
            .synchronizer
            .get_actual_secs_from_ticks(absolute_tick);
        let mut result = 0.0;
        for index in &self.active_vibratos {
            let note_vibrato = &self.vibratos[*index];
            let position =
                (seconds - note_vibrato.start_secs) / note_vibrato.duration_secs;
            result += note_vibrato.curve.evaluate(position);
        }
        result
    }
}

fn actual_curve(points: &[ControlPoint], base_value: f64, value_scale: f64) -> ParamCurve {
    let scale_curve = ScaleCurve::new(points);
    let mut positions: Vec<f64> = vec![0.0, 1.0];
    positions.extend(points.iter().map(|point| point.x));
    positions.sort_by(|left, right| {
        left.partial_cmp(right)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    positions.dedup_by(|left, right| *left == *right);
    ParamCurve::new(
        positions
            .into_iter()
            .map(|position| {
                Point::new(
                    py_round(position * VIBRATO_POSITION_SCALE),
                    py_round(base_value * scale_curve.value_at(position) * value_scale),
                )
            })
            .collect(),
    )
}

pub fn import_vibrato(vibrato: &Vibrato) -> VibratoParam {
    VibratoParam {
        start_percent: vibrato.start,
        end_percent: vibrato.end,
        is_anti_phase: (0.25..0.75).contains(&vibrato.phase),
        amplitude: actual_curve(&vibrato.points.amp, vibrato.amp as f64, 1.0),
        frequency: actual_curve(
            &vibrato.points.freq,
            vibrato.freq,
            VIBRATO_FREQUENCY_SCALE,
        ),
    }
}

fn export_control_points(curve: &ParamCurve) -> Vec<ControlPoint> {
    curve
        .points
        .iter()
        .filter(|point| {
            point.x != START_POINT_X && point.x != END_POINT_X && point.y != -100
        })
        .map(|point| ControlPoint {
            x: point.x as f64 / VIBRATO_POSITION_SCALE,
            y: point.y as f64,
        })
        .collect()
}

pub fn export_vibrato(vibrato: Option<&VibratoParam>, preserve: bool) -> Vibrato {
    let Some(vibrato) = vibrato.filter(|_| preserve) else {
        return Vibrato {
            start: 0.0,
            end: 1.0,
            amp: 0,
            freq: 0.0,
            phase: 0.0,
            offset: 0,
            points: VibratoPoints::default(),
        };
    };
    let amplitude = export_control_points(&vibrato.amplitude);
    let frequency = export_control_points(&vibrato.frequency);
    Vibrato {
        start: clamp(vibrato.start_percent, 0.0, 1.0),
        end: clamp(vibrato.end_percent, 0.0, 1.0),
        amp: i64::from(!amplitude.is_empty()),
        freq: if frequency.is_empty() {
            0.0
        } else {
            1.0 / VIBRATO_FREQUENCY_SCALE
        },
        phase: if vibrato.is_anti_phase { 0.5 } else { 0.0 },
        offset: 0,
        points: VibratoPoints {
            amp: amplitude,
            freq: frequency,
        },
    }
}
