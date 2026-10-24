//! Port of `libresvip.model.vocaloid.pitch_handler` together with the helpers
//! it needs from `libresvip.model.pitch_simulator`,
//! `libresvip.model.relative_pitch_curve`, `libresvip.model.portamento` and
//! `libresvip.core.time_interval` (`PiecewiseIntervalDict`).
//!
//! VPR only uses the pitch-bend conversion (no vibrato interval dictionaries),
//! so the vibrato branch of `to_absolute_pitch` is omitted.

use std::collections::HashMap;
use std::f64::consts::PI;

use libresvip_core::core::constants::{MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS, TICKS_IN_BEAT};
use libresvip_core::core::tick_counter::find_bar_index;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, ParamCurve, TimeSignature};
use libresvip_core::model::point::Point;

use super::controllers::{
    py_round, ControllerCurve, ControllerEvent, DEFAULT_PITCH_BEND_SENSITIVITY,
    MAX_PITCH_BEND_SENSITIVITY, PITCH_MAX_VALUE,
};

/// `MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS // 2`.
const PITCH_SECTION_BREAK_HALF: i64 = MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS / 2;

// ---------------------------------------------------------------------------
// PortamentoPitch
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpolationMode {
    Linear,
    Vocaloid,
}

#[derive(Clone, Copy, Debug)]
pub struct PortamentoPitch {
    pub max_inter_time_in_secs: f64,
    pub max_inter_time_percent: f64,
    pub mode: InterpolationMode,
    pub vocaloid_mode: bool,
}

impl PortamentoPitch {
    pub fn no_portamento() -> Self {
        Self {
            max_inter_time_in_secs: 0.0,
            max_inter_time_percent: 0.0,
            mode: InterpolationMode::Linear,
            vocaloid_mode: false,
        }
    }

    pub fn vocaloid_portamento() -> Self {
        Self {
            max_inter_time_in_secs: 0.05,
            max_inter_time_percent: 0.15,
            mode: InterpolationMode::Vocaloid,
            vocaloid_mode: true,
        }
    }
}

// ---------------------------------------------------------------------------
// PiecewiseIntervalDict
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum IntervalValue {
    Constant(f64),
    Portamento {
        start: (f64, f64),
        end: (f64, f64),
        mode: InterpolationMode,
    },
}

impl IntervalValue {
    fn evaluate(&self, x: f64) -> f64 {
        match *self {
            IntervalValue::Constant(value) => value,
            IntervalValue::Portamento { start, end, mode } => {
                let (x0, y0) = start;
                let (x1, y1) = end;
                let ratio = (x - x0) / (x1 - x0);
                let factor = match mode {
                    InterpolationMode::Linear => ratio,
                    InterpolationMode::Vocaloid => {
                        if ratio <= 0.5 {
                            (ratio * PI).sin() / 2.0
                        } else {
                            ratio
                        }
                    }
                };
                y0 + (y1 - y0) * factor
            }
        }
    }
}

/// `PiecewiseIntervalDict` — overlapping intervals are allowed; a lookup
/// returns the first interval (sorted by its lower bound) that contains `x`.
/// Storage/matching lives in the shared `IntervalDict`.
#[derive(Clone, Debug, Default)]
pub struct PiecewiseIntervalDict {
    dict: crate::interval_dict::IntervalDict<IntervalValue>,
}

impl PiecewiseIntervalDict {
    pub fn set_closedopen(&mut self, start: f64, end: f64, value: f64) {
        self.dict
            .set(start, end, true, false, IntervalValue::Constant(value));
    }

    pub fn set_portamento(
        &mut self,
        start: f64,
        end: f64,
        from: (f64, f64),
        to: (f64, f64),
        mode: InterpolationMode,
    ) {
        self.dict.set(
            start,
            end,
            true,
            false,
            IntervalValue::Portamento {
                start: from,
                end: to,
                mode,
            },
        );
    }

    /// `PiecewiseIntervalDict.get(x)` — the value of the first matching
    /// interval.
    pub fn get(&mut self, x: f64) -> Option<f64> {
        self.dict.get(x, |value, x| value.evaluate(x))
    }
}

// ---------------------------------------------------------------------------
// PitchSimulator
// ---------------------------------------------------------------------------

pub struct PitchSimulator {
    synchronizer: TimeSynchronizer,
    portamento: PortamentoPitch,
    interval_dict: PiecewiseIntervalDict,
}

impl PitchSimulator {
    pub fn new(
        synchronizer: TimeSynchronizer,
        portamento: PortamentoPitch,
        note_list: &[Note],
        time_signature_list: &[TimeSignature],
    ) -> Result<Self> {
        let mut simulator = Self {
            synchronizer,
            portamento,
            interval_dict: PiecewiseIntervalDict::default(),
        };
        simulator.build(note_list, time_signature_list)?;
        Ok(simulator)
    }

    fn vocaloid_max_portamento(&self, note: &Note) -> (f64, f64) {
        let max_portamento_percent = self.portamento.max_inter_time_percent;
        let mut max_portamento_ticks = max_portamento_percent * note.length as f64;
        if max_portamento_ticks >= 60.0 {
            max_portamento_ticks = 60.0;
        } else if note.length <= 120 {
            max_portamento_ticks = note.length as f64 / 2.0;
        }
        let max_portamento_time = self.synchronizer.get_duration_secs_from_ticks(
            (note.end_pos() as f64 - max_portamento_ticks * 1.4) as i64,
            (note.end_pos() as f64 - max_portamento_ticks * 0.4) as i64,
        );
        (max_portamento_ticks, max_portamento_time)
    }

    fn build(&mut self, note_list: &[Note], time_signature_list: &[TimeSignature]) -> Result<()> {
        if note_list.is_empty() {
            return Ok(());
        }
        let max_portamento_percent = self.portamento.max_inter_time_percent;
        let mut current_note = note_list[0].clone();

        let (mut max_portamento_ticks, mut max_portamento_time) = if self.portamento.vocaloid_mode {
            self.vocaloid_max_portamento(&current_note)
        } else {
            (0.0, self.portamento.max_inter_time_in_secs)
        };

        let mut current_head = self
            .synchronizer
            .get_actual_secs_from_ticks(current_note.start_pos);
        let mut current_dur = self
            .synchronizer
            .get_duration_secs_from_ticks(current_note.start_pos, current_note.end_pos());
        let mut current_portamento =
            (current_dur * max_portamento_percent).min(max_portamento_time);

        self.interval_dict
            .set_closedopen(0.0, current_head, current_note.key_number as f64);
        let mut prev_portamento_end = current_head;

        for next_note in &note_list[1..] {
            if current_note.end_pos() > next_note.start_pos {
                let bar = find_bar_index(time_signature_list, next_note.start_pos)?;
                return Err(LibreSvipError::msg(format!(
                    "Notes overlapped near bar {bar}"
                )));
            } else if self.portamento.vocaloid_mode
                && next_note.start_pos - current_note.end_pos()
                    >= MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS
            {
                max_portamento_time = 0.0;
            }
            let next_head = self
                .synchronizer
                .get_actual_secs_from_ticks(next_note.start_pos);
            let next_dur = self
                .synchronizer
                .get_duration_secs_from_ticks(next_note.start_pos, next_note.end_pos());
            if self.portamento.vocaloid_mode && next_note.start_pos > current_note.end_pos() {
                let current_tail = self
                    .synchronizer
                    .get_actual_secs_from_ticks(current_note.end_pos());
                if prev_portamento_end < current_tail {
                    self.interval_dict.set_closedopen(
                        prev_portamento_end,
                        current_tail,
                        current_note.key_number as f64,
                    );
                }
                current_note = next_note.clone();
                current_head = next_head;
                current_dur = next_dur;
                let (ticks, time) = self.vocaloid_max_portamento(&current_note);
                max_portamento_ticks = ticks;
                max_portamento_time = time;
                current_portamento =
                    (current_dur * max_portamento_percent).min(max_portamento_time);
                prev_portamento_end = current_head;
                continue;
            }
            let next_portamento = (next_dur * max_portamento_percent).min(max_portamento_time);
            let (interval, middle_time) = if self.portamento.vocaloid_mode {
                let base = if next_note.lyric == "-" {
                    current_note.end_pos() as f64
                } else {
                    (next_note.start_pos + current_note.end_pos()) as f64 / 2.0
                };
                let middle_pos = base - max_portamento_ticks * 0.4;
                let interval = self.synchronizer.get_duration_secs_from_ticks(
                    (middle_pos - max_portamento_ticks) as i64,
                    middle_pos as i64,
                );
                let middle_time = self
                    .synchronizer
                    .get_actual_secs_from_ticks(middle_pos as i64);
                (interval, middle_time)
            } else {
                (
                    (next_head - current_head - current_dur) / 2.0,
                    (next_head + current_head + current_dur) / 2.0,
                )
            };
            let (current_portamento_start, current_portamento_end) =
                if interval <= max_portamento_time {
                    (
                        middle_time - current_portamento,
                        middle_time + next_portamento,
                    )
                } else {
                    (
                        middle_time - max_portamento_time,
                        middle_time + max_portamento_time,
                    )
                };
            self.interval_dict.set_closedopen(
                prev_portamento_end,
                current_portamento_start,
                current_note.key_number as f64,
            );
            if current_note.key_number == next_note.key_number {
                self.interval_dict.set_closedopen(
                    current_portamento_start,
                    current_portamento_end,
                    current_note.key_number as f64,
                );
            } else if current_portamento_start < current_portamento_end {
                self.interval_dict.set_portamento(
                    current_portamento_start,
                    current_portamento_end,
                    (current_portamento_start, current_note.key_number as f64),
                    (current_portamento_end, next_note.key_number as f64),
                    self.portamento.mode,
                );
            }
            current_note = next_note.clone();
            current_head = next_head;
            current_dur = next_dur;
            current_portamento = next_portamento;
            prev_portamento_end = current_portamento_end;
            if self.portamento.vocaloid_mode {
                let (ticks, time) = self.vocaloid_max_portamento(&current_note);
                max_portamento_ticks = ticks;
                max_portamento_time = time;
            }
        }
        self.interval_dict.set_closedopen(
            prev_portamento_end,
            f64::INFINITY,
            current_note.key_number as f64,
        );
        Ok(())
    }

    pub fn pitch_at_ticks(&mut self, ticks: i64) -> Option<f64> {
        let secs = self.synchronizer.get_actual_secs_from_ticks(ticks);
        self.pitch_at_secs(secs)
    }

    pub fn pitch_at_ticks_batch(&mut self, ticks_list: &[i64]) -> Vec<Option<f64>> {
        if ticks_list.is_empty() {
            return Vec::new();
        }
        let secs_list = self
            .synchronizer
            .get_actual_secs_from_ticks_batch(ticks_list);
        secs_list
            .into_iter()
            .map(|secs| self.pitch_at_secs(secs))
            .collect()
    }

    pub fn pitch_at_secs(&mut self, secs: f64) -> Option<f64> {
        // `if value := self.interval_dict.get(secs)` — a zero value is falsy.
        match self.interval_dict.get(secs) {
            Some(value) if value != 0.0 => Some(value * 100.0),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// RelativePitchCurve
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct RelativePitchCurve {
    pub first_bar_length: i64,
    pub lower_bound: f64,
    pub upper_bound_is_inf: bool,
    pub pitch_interval: i64,
    pub is_staircase: bool,
}

impl Default for RelativePitchCurve {
    fn default() -> Self {
        Self {
            first_bar_length: TICKS_IN_BEAT * 4,
            lower_bound: 0.0,
            upper_bound_is_inf: true,
            pitch_interval: 5,
            is_staircase: true,
        }
    }
}

impl RelativePitchCurve {
    pub fn new(first_bar_length: i64) -> Self {
        Self {
            first_bar_length,
            ..Self::default()
        }
    }

    pub fn to_absolute(&self, points: &[Point], simulator: &mut PitchSimulator) -> ParamCurve {
        ParamCurve::new(self.convert_relativity(points, simulator, true))
    }

    pub fn from_absolute(&self, points: &[Point], simulator: &mut PitchSimulator) -> Vec<Point> {
        self.convert_relativity(points, simulator, false)
    }

    fn convert_relativity(
        &self,
        points: &[Point],
        simulator: &mut PitchSimulator,
        to_absolute: bool,
    ) -> Vec<Point> {
        let pitch_offset = if to_absolute {
            0
        } else {
            -self.first_bar_length
        };
        let converted_offset = if to_absolute {
            self.first_bar_length
        } else {
            -self.first_bar_length
        };
        let point_tick_positions: Vec<i64> = points
            .iter()
            .map(|point| point.x + pitch_offset)
            .collect();
        let pitch_values = simulator.pitch_at_ticks_batch(&point_tick_positions);
        // `dict(zip(positions, values))`: the last value for a position wins.
        // A hash map keeps the interpolation loop linear; a Vec scanned per
        // lookup made the conversion quadratic in the song length.
        let mut pitch_value_map: HashMap<i64, Option<f64>> =
            HashMap::with_capacity(point_tick_positions.len());
        for (position, value) in point_tick_positions.iter().zip(pitch_values.iter()) {
            pitch_value_map.insert(*position, *value);
        }
        let mut lookup = |position: i64, simulator: &mut PitchSimulator| -> Option<f64> {
            if let Some(value) = pitch_value_map.get(&position) {
                return *value;
            }
            let value = simulator.pitch_at_ticks(position);
            pitch_value_map.insert(position, value);
            value
        };

        let mut converted_data: Vec<Point> = Vec::new();
        let mut prev_x: Option<i64> = None;
        let mut prev_y: Option<f64> = None;
        for point in points {
            let pos = point.x + pitch_offset;
            let cur_x = point.x + converted_offset;
            let base_key = lookup(pos, simulator);
            let (y, rel_y): (Option<f64>, Option<f64>) = match base_key {
                None => (None, None),
                Some(base_key) => {
                    if !to_absolute && point.y == -100 {
                        if point.x == -192_000 || point.x == 1_073_741_823 {
                            continue;
                        }
                        (Some(0.0), Some(0.0))
                    } else {
                        let rel_y = if to_absolute {
                            point.y as f64
                        } else {
                            point.y as f64 - base_key
                        };
                        let y = if to_absolute { rel_y + base_key } else { rel_y };
                        (Some(y), Some(rel_y))
                    }
                }
            };
            if let (Some(y), Some(prev_x_value), Some(prev_y_value)) = (y, prev_x, prev_y) {
                if !converted_data.is_empty() && cur_x - prev_x_value > self.pitch_interval {
                    let mut tick = prev_x_value + self.pitch_interval;
                    while tick < cur_x {
                        let tick_key = if to_absolute {
                            lookup(tick - self.first_bar_length, simulator)
                        } else {
                            lookup(tick, simulator)
                        };
                        if to_absolute {
                            let Some(tick_key) = tick_key else { break };
                            let interpolated_rel_y = if self.is_staircase || rel_y.is_none() {
                                prev_y_value
                            } else {
                                let rel_y = rel_y.unwrap_or(0.0);
                                prev_y_value
                                    + (rel_y - prev_y_value) * (tick - prev_x_value) as f64
                                        / (cur_x - prev_x_value) as f64
                            };
                            converted_data.push(Point::new(
                                tick,
                                py_round(interpolated_rel_y + tick_key),
                            ));
                        } else if !(self.is_staircase && prev_y_value == y) {
                            converted_data.push(Point::new(
                                tick,
                                py_round(
                                    prev_y_value
                                        + (y - prev_y_value) * (tick - prev_x_value) as f64
                                            / (cur_x - prev_x_value) as f64,
                                ),
                            ));
                        }
                        tick += self.pitch_interval;
                    }
                }
            }
            if let Some(y) = y {
                if to_absolute && prev_y.is_none() {
                    converted_data.push(Point::new(cur_x, -100));
                }
                converted_data.push(Point::new(cur_x, py_round(y)));
                if to_absolute {
                    if point.y != 0 {
                        prev_y = rel_y;
                    } else {
                        converted_data.push(Point::new(cur_x, -100));
                        prev_y = None;
                    }
                } else {
                    prev_y = if point.y == -100 { None } else { Some(y) };
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
            if self.upper_bound_is_inf {
                let mut last = *converted_data.last().unwrap_or(&Point::default());
                last.y = -100;
                converted_data.push(last);
                converted_data.push(Point::end_point());
            }
        }
        converted_data
    }
}

// ---------------------------------------------------------------------------
// PitchBendData / VocaloidPitchHandler
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PitchBendData {
    pub pit: ControllerCurve,
    pub pbs: ControllerCurve,
}

impl PitchBendData {
    pub fn is_empty(&self) -> bool {
        self.pit.is_empty() && self.pbs.is_empty()
    }
}

pub struct VocaloidPitchHandler {
    pub synchronizer: TimeSynchronizer,
    pub note_list: Vec<Note>,
    pub time_signature_list: Vec<TimeSignature>,
    pub first_bar_length: i64,
}

impl VocaloidPitchHandler {
    pub fn new(
        synchronizer: TimeSynchronizer,
        note_list: &[Note],
        time_signature_list: &[TimeSignature],
        first_bar_length: i64,
    ) -> Self {
        Self {
            synchronizer,
            note_list: note_list.to_vec(),
            time_signature_list: time_signature_list.to_vec(),
            first_bar_length,
        }
    }

    pub fn combine_pit_pbs(&self, pit: &ControllerCurve, pbs: &ControllerCurve) -> Vec<(i64, f64)> {
        let mut all_positions: Vec<i64> = pit
            .events
            .iter()
            .chain(pbs.events.iter())
            .map(|event| event.pos)
            .collect();
        all_positions.sort_unstable();
        all_positions.dedup();
        if all_positions.is_empty() {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(all_positions.len());
        // `ControllerCurve.get_value_at` never returns None, so Python's
        // `if pbs_value is not None` guard always passes and the initial value
        // is overwritten on every iteration.
        let mut current_pbs = DEFAULT_PITCH_BEND_SENSITIVITY;
        let _ = current_pbs;
        for pos in all_positions {
            // `ControllerCurve.get_value_at` never returns None, so Python's
            // `if pbs_value is not None` guard always passes.
            let pbs_value = pbs.get_value_at(pos);
            current_pbs = pbs_value.clamp(1, MAX_PITCH_BEND_SENSITIVITY);
            let pit_value = pit.get_value_at(pos);
            let denominator = if pit_value > 0 {
                PITCH_MAX_VALUE as f64
            } else {
                (PITCH_MAX_VALUE + 1) as f64
            };
            let pitch_cents = (pit_value as f64 / denominator) * current_pbs as f64 * 100.0;
            result.push((pos, pitch_cents));
        }
        result
    }

    /// `to_absolute_pitch(pitch_data_list, part_offsets)`.
    pub fn to_absolute_pitch(
        &self,
        pitch_data_list: &[PitchBendData],
        part_offsets: Option<&[i64]>,
    ) -> Result<Option<ParamCurve>> {
        if pitch_data_list.is_empty() {
            return Ok(None);
        }
        let offsets: Vec<i64> = match part_offsets {
            Some(offsets) => offsets.to_vec(),
            None => vec![0; pitch_data_list.len()],
        };
        let mut all_pitch_points: Vec<(i64, f64)> = Vec::new();
        for (data, offset) in pitch_data_list.iter().zip(offsets.iter()) {
            if data.is_empty() {
                continue;
            }
            for (pos, pitch) in self.combine_pit_pbs(&data.pit, &data.pbs) {
                all_pitch_points.push((pos + offset, pitch));
            }
        }
        if all_pitch_points.is_empty() {
            return Ok(None);
        }
        all_pitch_points.sort_by_key(|point| point.0);
        // Positions are sorted, so a duplicate can only be the previous entry.
        let mut unique_points: Vec<(i64, f64)> = Vec::new();
        for (pos, pitch) in all_pitch_points {
            match unique_points.last_mut() {
                Some(entry) if entry.0 == pos => entry.1 = pitch,
                _ => unique_points.push((pos, pitch)),
            }
        }
        let points_data: Vec<Point> = unique_points
            .iter()
            .map(|(pos, pitch)| Point::new(*pos, py_round(*pitch)))
            .collect();
        if points_data.is_empty() || self.note_list.is_empty() {
            return Ok(None);
        }
        let mut simulator = PitchSimulator::new(
            self.synchronizer.clone(),
            PortamentoPitch::vocaloid_portamento(),
            &self.note_list,
            &self.time_signature_list,
        )?;
        let relative = RelativePitchCurve::new(self.first_bar_length);
        Ok(Some(relative.to_absolute(&points_data, &mut simulator)))
    }

    /// `from_absolute_pitch`.
    pub fn from_absolute_pitch(&self, pitch: &ParamCurve) -> Result<PitchBendData> {
        let empty = || PitchBendData {
            pit: ControllerCurve::new("pitch_bend"),
            pbs: ControllerCurve::new("pitch_bend_sens"),
        };
        if pitch.points.is_empty() {
            return Ok(empty());
        }
        let mut simulator = PitchSimulator::new(
            self.synchronizer.clone(),
            PortamentoPitch::vocaloid_portamento(),
            &self.note_list,
            &self.time_signature_list,
        )?;
        let relative = RelativePitchCurve::new(self.first_bar_length);
        let relative_data = relative.from_absolute(&pitch.points, &mut simulator);
        if relative_data.is_empty() {
            return Ok(empty());
        }

        let mut pit_events: Vec<ControllerEvent> = Vec::new();
        let mut pbs_events: Vec<ControllerEvent> = Vec::new();
        for section in Self::split_into_sections(&relative_data) {
            if section.is_empty() {
                continue;
            }
            let max_abs_pitch = section
                .iter()
                .map(|point| (point.y as f64).abs())
                .fold(0.0f64, f64::max)
                / 100.0;
            let mut optimal_pbs = max_abs_pitch.ceil() as i64;
            optimal_pbs = optimal_pbs.min(MAX_PITCH_BEND_SENSITIVITY);
            optimal_pbs = optimal_pbs.max(DEFAULT_PITCH_BEND_SENSITIVITY);
            if optimal_pbs != DEFAULT_PITCH_BEND_SENSITIVITY {
                pbs_events.push(ControllerEvent {
                    pos: section[0].x,
                    value: optimal_pbs,
                });
                pbs_events.push(ControllerEvent {
                    pos: section[section.len() - 1].x + PITCH_SECTION_BREAK_HALF,
                    value: DEFAULT_PITCH_BEND_SENSITIVITY,
                });
            }
            for point in &section {
                let value = (point.y as f64 / 100.0 * PITCH_MAX_VALUE as f64
                    / optimal_pbs as f64)
                    .clamp((-PITCH_MAX_VALUE - 1) as f64, PITCH_MAX_VALUE as f64);
                pit_events.push(ControllerEvent {
                    pos: point.x,
                    value: value as i64,
                });
            }
        }

        Ok(PitchBendData {
            pit: ControllerCurve::with_events("pitch_bend", pit_events, 0, -8192, 8191),
            pbs: ControllerCurve::with_events(
                "pitch_bend_sens",
                pbs_events,
                DEFAULT_PITCH_BEND_SENSITIVITY,
                1,
                MAX_PITCH_BEND_SENSITIVITY,
            ),
        })
    }

    fn split_into_sections(points: &[Point]) -> Vec<Vec<Point>> {
        if points.is_empty() {
            return Vec::new();
        }
        let mut sections: Vec<Vec<Point>> = vec![Vec::new()];
        let mut current_pos = 0i64;
        for point in points {
            let last = sections.last_mut().expect("sections is never empty");
            if last.is_empty()
                || point.x - current_pos < MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS
            {
                last.push(*point);
            } else {
                sections.push(vec![*point]);
            }
            current_pos = point.x;
        }
        sections.retain(|section| !section.is_empty());
        sections
    }
}
