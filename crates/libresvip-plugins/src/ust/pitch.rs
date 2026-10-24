//! Port of `libresvip.model.pitch_simulator` (the no-portamento path used by
//! the UST plugin) and `libresvip.model.relative_pitch_curve`.

use std::collections::HashMap;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{Note, ParamCurve, TimeSignature};
use libresvip_core::model::point::Point;
use libresvip_core::{LibreSvipError, Result};

use super::math::{linear, round_int};

/// `PortamentoPitch` — only the two constructors used by the plugins.
#[derive(Clone, Copy, Debug)]
pub struct Portamento {
    pub max_inter_time_in_secs: f64,
    pub max_inter_time_percent: f64,
    pub vocaloid_mode: bool,
}

impl Portamento {
    /// `PortamentoPitch.no_portamento()`.
    pub fn no_portamento() -> Self {
        Self {
            max_inter_time_in_secs: 0.0,
            max_inter_time_percent: 0.0,
            vocaloid_mode: false,
        }
    }

    /// `PortamentoPitch.vocaloid_portamento()` — unused by the two plugins that
    /// consume this module, kept for parity with the Python model.
    #[allow(dead_code)]
    pub fn vocaloid_portamento() -> Self {
        Self {
            max_inter_time_in_secs: 0.05,
            max_inter_time_percent: 0.15,
            vocaloid_mode: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum IntervalValue {
    Key(i64),
    /// `functools.partial(inter_func, start=..., end=...)` — portamento ramp.
    Ramp { start: (f64, f64), end: (f64, f64) },
}

/// One half-open `[start, end)` stretch of the interval dictionary.
#[derive(Clone, Copy, Debug)]
struct Segment {
    start: f64,
    end: f64,
    value: IntervalValue,
}

/// Port of the parts of `PitchSimulator` used by the plugins.
#[derive(Clone, Debug, Default)]
pub struct PitchSimulator {
    segments: Vec<Segment>,
}

impl PitchSimulator {
    /// `PitchSimulator(synchronizer, portamento, note_list, time_signature_list)`
    /// — raises `NotesOverlappedError` for overlapping notes.
    pub fn new(
        synchronizer: &TimeSynchronizer,
        portamento: Portamento,
        note_list: &[Note],
        time_signature_list: &[TimeSignature],
    ) -> Result<Self> {
        let mut segments: Vec<Segment> = Vec::new();
        if note_list.is_empty() {
            return Ok(Self { segments });
        }
        let max_portamento_percent = portamento.max_inter_time_percent;
        let mut current_note = &note_list[0];
        let mut max_portamento_ticks = 0.0f64;
        let mut max_portamento_time;
        if portamento.vocaloid_mode {
            let bounds = portamento_bounds(synchronizer, portamento, current_note);
            max_portamento_ticks = bounds.0;
            max_portamento_time = bounds.1;
        } else {
            max_portamento_time = portamento.max_inter_time_in_secs;
        }
        let mut current_head = synchronizer.get_actual_secs_from_ticks(current_note.start_pos);
        let mut current_dur = synchronizer
            .get_duration_secs_from_ticks(current_note.start_pos, current_note.end_pos());
        let mut current_portamento =
            (current_dur * max_portamento_percent).min(max_portamento_time);
        push_segment(
            &mut segments,
            0.0,
            current_head,
            IntervalValue::Key(current_note.key_number),
        );
        let mut prev_portamento_end = current_head;
        for next_note in &note_list[1..] {
            if current_note.end_pos() > next_note.start_pos {
                let bar = libresvip_core::core::tick_counter::find_bar_index(
                    time_signature_list,
                    next_note.start_pos,
                )
                .unwrap_or(0);
                return Err(LibreSvipError::msg(format!(
                    "Notes overlapped near bar {bar}"
                )));
            } else if portamento.vocaloid_mode
                && next_note.start_pos - current_note.end_pos()
                    >= libresvip_core::core::constants::MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS
            {
                max_portamento_time = 0.0;
            }
            let next_head = synchronizer.get_actual_secs_from_ticks(next_note.start_pos);
            let next_dur = synchronizer
                .get_duration_secs_from_ticks(next_note.start_pos, next_note.end_pos());
            if portamento.vocaloid_mode && next_note.start_pos > current_note.end_pos() {
                let current_tail = synchronizer.get_actual_secs_from_ticks(current_note.end_pos());
                if prev_portamento_end < current_tail {
                    push_segment(
                        &mut segments,
                        prev_portamento_end,
                        current_tail,
                        IntervalValue::Key(current_note.key_number),
                    );
                }
                current_note = next_note;
                current_head = next_head;
                current_dur = next_dur;
                let bounds = portamento_bounds(synchronizer, portamento, current_note);
                max_portamento_ticks = bounds.0;
                max_portamento_time = bounds.1;
                current_portamento =
                    (current_dur * max_portamento_percent).min(max_portamento_time);
                prev_portamento_end = current_head;
                continue;
            }
            let next_portamento = (next_dur * max_portamento_percent).min(max_portamento_time);
            let (interval, middle_time) = if portamento.vocaloid_mode {
                let middle_pos = if next_note.lyric == "-" {
                    current_note.end_pos()
                } else {
                    (next_note.start_pos + current_note.end_pos()) / 2
                };
                let middle_pos = middle_pos as f64 - max_portamento_ticks * 0.4;
                let interval = synchronizer.get_duration_secs_from_ticks(
                    (middle_pos - max_portamento_ticks) as i64,
                    middle_pos as i64,
                );
                let middle_time = synchronizer.get_actual_secs_from_ticks(middle_pos as i64);
                (interval, middle_time)
            } else {
                (
                    (next_head - current_head - current_dur) / 2.0,
                    (next_head + current_head + current_dur) / 2.0,
                )
            };
            let (current_portamento_start, current_portamento_end) =
                if interval <= max_portamento_time {
                    (middle_time - current_portamento, middle_time + next_portamento)
                } else {
                    (middle_time - max_portamento_time, middle_time + max_portamento_time)
                };
            push_segment(
                &mut segments,
                prev_portamento_end,
                current_portamento_start,
                IntervalValue::Key(current_note.key_number),
            );
            if current_note.key_number == next_note.key_number {
                push_segment(
                    &mut segments,
                    current_portamento_start,
                    current_portamento_end,
                    IntervalValue::Key(current_note.key_number),
                );
            } else if current_portamento_start < current_portamento_end {
                push_segment(
                    &mut segments,
                    current_portamento_start,
                    current_portamento_end,
                    IntervalValue::Ramp {
                        start: (current_portamento_start, current_note.key_number as f64),
                        end: (current_portamento_end, next_note.key_number as f64),
                    },
                );
            }
            current_note = next_note;
            current_head = next_head;
            current_dur = next_dur;
            current_portamento = next_portamento;
            prev_portamento_end = current_portamento_end;
            if portamento.vocaloid_mode {
                let bounds = portamento_bounds(synchronizer, portamento, current_note);
                max_portamento_ticks = bounds.0;
                max_portamento_time = bounds.1;
            }
        }
        push_segment(
            &mut segments,
            prev_portamento_end,
            f64::INFINITY,
            IntervalValue::Key(current_note.key_number),
        );
        Ok(Self { segments })
    }

    fn value_at(&self, secs: f64) -> Option<f64> {
        for segment in &self.segments {
            if secs >= segment.start && secs < segment.end {
                return Some(match segment.value {
                    IntervalValue::Key(key) => key as f64,
                    IntervalValue::Ramp { start, end } => linear(secs, start, end),
                });
            }
        }
        None
    }

    /// `pitch_at_ticks` — `None` where the simulator has no value.
    pub fn pitch_at_ticks(&self, synchronizer: &TimeSynchronizer, ticks: i64) -> Option<f64> {
        self.pitch_at_secs(synchronizer.get_actual_secs_from_ticks(ticks))
    }

    /// `pitch_at_ticks_batch`, which is equivalent to mapping the scalar call.
    pub fn pitch_at_ticks_batch(
        &self,
        synchronizer: &TimeSynchronizer,
        ticks_list: &[i64],
    ) -> Vec<Option<f64>> {
        synchronizer
            .get_actual_secs_from_ticks_batch(ticks_list)
            .into_iter()
            .map(|secs| self.pitch_at_secs(secs))
            .collect()
    }

    /// `_pitch_at_secs_with_ordered_lookup`.
    pub fn pitch_at_secs(&self, secs: f64) -> Option<f64> {
        match self.value_at(secs) {
            // Fixed deviation from LibreSVIP 2.9.2: the reference relied on the
            // Python truthiness of the looked-up value, so key 0 (C-1) counted
            // as absent. It is a real note.
            Some(value) => Some(value * 100.0),
            _ => None,
        }
    }
}

/// `vocaloid_max_portamento` for `PortamentoPitch.vocaloid_portamento()`.
fn portamento_bounds(
    synchronizer: &TimeSynchronizer,
    portamento: Portamento,
    note: &Note,
) -> (f64, f64) {
    if !portamento.vocaloid_mode {
        return (0.0, portamento.max_inter_time_in_secs);
    }
    let mut max_portamento_ticks = portamento.max_inter_time_percent * note.length as f64;
    if max_portamento_ticks >= 60.0 {
        max_portamento_ticks = 60.0;
    } else if note.length <= 120 {
        max_portamento_ticks = note.length as f64 / 2.0;
    }
    let end = note.end_pos() as f64;
    let max_portamento_time = synchronizer.get_duration_secs_from_ticks(
        (end - max_portamento_ticks * 1.4) as i64,
        (end - max_portamento_ticks * 0.4) as i64,
    );
    (max_portamento_ticks, max_portamento_time)
}

fn push_segment(segments: &mut Vec<Segment>, start: f64, end: f64, value: IntervalValue) {
    // `portion.closedopen(start, end)` is empty when end <= start; assigning an
    // empty interval to a `portion.IntervalDict` is a no-op.
    if !(end > start) {
        return;
    }
    segments.push(Segment { start, end, value });
}

/// Port of `RelativePitchCurve`.
#[derive(Clone, Copy, Debug)]
pub struct RelativePitchCurve {
    pub first_bar_length: i64,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub pitch_interval: i64,
    pub is_staircase: bool,
}

impl Default for RelativePitchCurve {
    fn default() -> Self {
        Self {
            first_bar_length: libresvip_core::model::base::TICKS_IN_BEAT * 4,
            lower_bound: 0.0,
            upper_bound: f64::INFINITY,
            pitch_interval: 5,
            is_staircase: true,
        }
    }
}

impl RelativePitchCurve {
    /// `to_absolute`.
    pub fn to_absolute(
        &self,
        points: &[Point],
        synchronizer: &TimeSynchronizer,
        pitch_simulator: &PitchSimulator,
    ) -> ParamCurve {
        ParamCurve::new(self.convert_relativity(points, synchronizer, pitch_simulator, true))
    }

    /// `from_absolute` — unused by the UST/USTX plugins, kept for parity.
    #[allow(dead_code)]
    pub fn from_absolute(
        &self,
        points: &[Point],
        synchronizer: &TimeSynchronizer,
        pitch_simulator: &PitchSimulator,
    ) -> Vec<Point> {
        self.convert_relativity(points, synchronizer, pitch_simulator, false)
    }

    fn convert_relativity(
        &self,
        points: &[Point],
        synchronizer: &TimeSynchronizer,
        pitch_simulator: &PitchSimulator,
        to_absolute: bool,
    ) -> Vec<Point> {
        let pitch_offset: i64 = if to_absolute { 0 } else { -self.first_bar_length };
        let converted_offset: i64 = if to_absolute {
            self.first_bar_length
        } else {
            -self.first_bar_length
        };
        let point_tick_positions: Vec<i64> =
            points.iter().map(|point| point.x + pitch_offset).collect();
        let pitch_values =
            pitch_simulator.pitch_at_ticks_batch(synchronizer, &point_tick_positions);
        let mut pitch_value_map: HashMap<i64, Option<f64>> = HashMap::new();
        for (tick, value) in point_tick_positions.iter().zip(pitch_values.iter()) {
            pitch_value_map.insert(*tick, *value);
        }

        let mut converted_data: Vec<Point> = Vec::new();
        let mut prev_x: Option<i64> = None;
        let mut prev_y: Option<f64> = None;
        for point in points {
            let pos = point.x + pitch_offset;
            let cur_x = point.x + converted_offset;
            let base_key = pitch_value(
                &mut pitch_value_map,
                synchronizer,
                pitch_simulator,
                pos,
            );
            let mut rel_y: Option<f64> = None;
            let y: Option<f64>;
            if base_key.is_none() {
                y = None;
            } else if !to_absolute && point.y == -100 {
                if point.x == -192000 || point.x == 1_073_741_823 {
                    continue;
                }
                y = Some(0.0);
                rel_y = y;
            } else {
                let base = base_key.unwrap_or(0.0);
                let ry = if to_absolute {
                    point.y as f64
                } else {
                    point.y as f64 - base
                };
                rel_y = Some(ry);
                y = Some(if to_absolute { ry + base } else { ry });
            }
            if let (Some(y_value), Some(prev_x_value), Some(prev_y_value)) = (y, prev_x, prev_y) {
                if !converted_data.is_empty() && cur_x - prev_x_value > self.pitch_interval {
                    let mut tick = prev_x_value + self.pitch_interval;
                    while tick < cur_x {
                        let lookup = if to_absolute {
                            tick - self.first_bar_length
                        } else {
                            tick
                        };
                        let Some(tick_key) = pitch_value(
                            &mut pitch_value_map,
                            synchronizer,
                            pitch_simulator,
                            lookup,
                        ) else {
                            break;
                        };
                        if to_absolute {
                            let interpolated_rel_y = if self.is_staircase || rel_y.is_none() {
                                prev_y_value
                            } else {
                                prev_y_value
                                    + (rel_y.unwrap_or(0.0) - prev_y_value)
                                        * (tick - prev_x_value) as f64
                                        / (cur_x - prev_x_value) as f64
                            };
                            converted_data
                                .push(Point::new(tick, round_int(interpolated_rel_y + tick_key)));
                        } else if !(self.is_staircase && prev_y_value == y_value) {
                            converted_data.push(Point::new(
                                tick,
                                round_int(
                                    prev_y_value
                                        + (y_value - prev_y_value) * (tick - prev_x_value) as f64
                                            / (cur_x - prev_x_value) as f64,
                                ),
                            ));
                        }
                        tick += self.pitch_interval;
                    }
                }
            }
            if let Some(y_value) = y {
                if to_absolute && prev_y.is_none() {
                    converted_data.push(Point::new(cur_x, -100));
                }
                converted_data.push(Point::new(cur_x, round_int(y_value)));
                if to_absolute {
                    if point.y != 0 {
                        prev_y = rel_y;
                    } else {
                        converted_data.push(Point::new(cur_x, -100));
                        prev_y = None;
                    }
                } else {
                    prev_y = if point.y == -100 { None } else { Some(y_value) };
                }
            } else {
                prev_y = None;
            }
            prev_x = Some(cur_x);
        }
        if !converted_data.is_empty() && to_absolute {
            if self.lower_bound == 0.0 {
                converted_data.insert(0, Point::start_point());
            }
            if self.upper_bound == f64::INFINITY {
                let mut last = *converted_data.last().unwrap_or(&Point::default());
                last.y = -100;
                converted_data.push(last);
                converted_data.push(Point::end_point());
            }
        }
        converted_data
    }
}

/// `get_pitch_value`: cache lookup, falling back to a direct simulator query.
fn pitch_value(
    cache: &mut HashMap<i64, Option<f64>>,
    synchronizer: &TimeSynchronizer,
    pitch_simulator: &PitchSimulator,
    tick: i64,
) -> Option<f64> {
    if let Some(value) = cache.get(&tick) {
        return *value;
    }
    let value = pitch_simulator.pitch_at_ticks(synchronizer, tick);
    cache.insert(tick, value);
    value
}
