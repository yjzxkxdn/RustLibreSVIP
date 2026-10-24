//! Pitch curve conversion helpers.
//!
//! Port of `libresvip.model.relative_pitch_curve`, `libresvip.model.pitch_simulator`
//! and `libresvip.model.portamento`, limited to the parts the `mid` and
//! `ufdata` plugins use.
//!
//! The Python implementations store pitch intervals in a
//! `portion.IntervalDict`; assignment overwrites overlapping intervals, so a
//! point query always resolves to the most recently assigned interval that
//! covers it. [`IntervalMap`] models exactly that (a stack of intervals queried
//! newest-first), which is equivalent for point lookups.

use std::collections::HashMap;


use libresvip_core::core::constants::MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS;
use libresvip_core::core::tick_counter::find_bar_index;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, ParamCurve, TimeSignature};
use libresvip_core::model::point::{Point, END_POINT_X, START_POINT_X};

/// `PITCH_MIN_VALUE` from `libresvip.utils.binary.midi`.
pub const PITCH_MIN_VALUE: i64 = -8192;
/// `PITCH_MAX_VALUE` from `libresvip.utils.binary.midi`.
pub const PITCH_MAX_VALUE: i64 = 8191;
/// `DEFAULT_PITCH_BEND_SENSITIVITY`.
pub const DEFAULT_PITCH_BEND_SENSITIVITY: i64 = 2;
/// `MAX_PITCH_BEND_SENSITIVITY`.
pub const MAX_PITCH_BEND_SENSITIVITY: i64 = 24;

/// Python's `round()` (ties to even) as `i64`.

pub use libresvip_core::core::compat::py_round as round_to_i64;

/// `libresvip.utils.music_math.clamp`.
fn clamp(value: f64, lower: f64, upper: f64) -> f64 {
    value.max(lower).min(upper)
}

// ---------------------------------------------------------------------------
// Portamento
// ---------------------------------------------------------------------------

/// `libresvip.model.portamento.PortamentoPitch`.
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

    /// `PortamentoPitch.vocaloid_portamento()`.
    pub fn vocaloid_portamento() -> Self {
        Self {
            max_inter_time_in_secs: 0.05,
            max_inter_time_percent: 0.15,
            vocaloid_mode: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Interval map
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum IntervalValue {
    Constant(f64),
    /// `inter_func` with `start=(x0, y0)` and `end=(x1, y1)`.
    Linear {
        start: f64,
        start_value: f64,
        end: f64,
        end_value: f64,
        vocaloid: bool,
    },
}

impl IntervalValue {
    fn evaluate(&self, position: f64) -> f64 {
        match *self {
            IntervalValue::Constant(value) => value,
            IntervalValue::Linear {
                start,
                start_value,
                end,
                end_value,
                vocaloid,
            } => {
                let ratio = (position - start) / (end - start);
                // `linear_interpolation` / `vocaloid_interpolation`.
                let factor = if vocaloid {
                    if ratio <= 0.5 {
                        (ratio * std::f64::consts::PI).sin() / 2.0
                    } else {
                        ratio
                    }
                } else {
                    ratio
                };
                start_value + (end_value - start_value) * factor
            }
        }
    }

    /// Python truthiness: `0` is falsy, a `functools.partial` is always truthy.
    fn is_truthy(&self) -> bool {
        match *self {
            IntervalValue::Constant(value) => value != 0.0,
            IntervalValue::Linear { .. } => true,
        }
    }
}

/// Half-open `[start, end)` intervals, newest assignment wins on lookup.
#[derive(Clone, Debug, Default)]
struct IntervalMap {
    entries: Vec<(f64, f64, IntervalValue)>,
}

impl IntervalMap {
    fn set(&mut self, start: f64, end: f64, value: IntervalValue) {
        // `portion.closedopen(a, b)` is empty when `a >= b`; assignment skips it.
        if !(start < end) {
            return;
        }
        self.entries.push((start, end, value));
    }

    fn get(&self, position: f64) -> Option<IntervalValue> {
        self.entries
            .iter()
            .rev()
            .find(|(start, end, _)| position >= *start && position < *end)
            .map(|(_, _, value)| *value)
    }
}

// ---------------------------------------------------------------------------
// PitchSimulator
// ---------------------------------------------------------------------------

/// `libresvip.model.pitch_simulator.PitchSimulator`.
pub struct PitchSimulator {
    synchronizer: TimeSynchronizer,
    interval_dict: IntervalMap,
}

impl PitchSimulator {
    /// Python's `__post_init__`; raises `NotesOverlappedError` on overlap.
    pub fn new(
        synchronizer: &TimeSynchronizer,
        portamento: Portamento,
        note_list: &[Note],
        time_signature_list: &[TimeSignature],
    ) -> Result<Self> {
        let mut interval_dict = IntervalMap::default();
        if note_list.is_empty() {
            return Ok(Self {
                synchronizer: synchronizer.clone(),
                interval_dict,
            });
        }
        if synchronizer.tempo_list.is_empty() {
            // Python would raise IndexError inside `TimeSynchronizer`.
            return Err(LibreSvipError::msg("song tempo list must not be empty"));
        }
        let max_portamento_percent = portamento.max_inter_time_percent;
        let vocaloid_max_portamento = |note: &Note| -> (f64, f64) {
            let mut max_portamento_ticks = max_portamento_percent * note.length as f64;
            if max_portamento_ticks >= 60.0 {
                max_portamento_ticks = 60.0;
            } else if note.length <= 120 {
                max_portamento_ticks = note.length as f64 / 2.0;
            }
            let end_pos = note.end_pos() as f64;
            let max_portamento_time = synchronizer.get_duration_secs_from_ticks(
                (end_pos - max_portamento_ticks * 1.4) as i64,
                (end_pos - max_portamento_ticks * 0.4) as i64,
            );
            (max_portamento_ticks, max_portamento_time)
        };

        let mut current_note = &note_list[0];
        let mut max_portamento_ticks: f64;
        let mut max_portamento_time: f64;
        if portamento.vocaloid_mode {
            let (ticks, time) = vocaloid_max_portamento(current_note);
            max_portamento_ticks = ticks;
            max_portamento_time = time;
        } else {
            max_portamento_ticks = 0.0;
            max_portamento_time = portamento.max_inter_time_in_secs;
        }
        let mut current_head = synchronizer.get_actual_secs_from_ticks(current_note.start_pos);
        let mut current_dur = synchronizer
            .get_duration_secs_from_ticks(current_note.start_pos, current_note.end_pos());
        let mut current_portamento = (current_dur * max_portamento_percent).min(max_portamento_time);
        interval_dict.set(
            0.0,
            current_head,
            IntervalValue::Constant(current_note.key_number as f64),
        );
        let mut prev_portamento_end = current_head;
        for next_note in &note_list[1..] {
            if current_note.end_pos() > next_note.start_pos {
                let bar_index = find_bar_index(time_signature_list, next_note.start_pos)?;
                return Err(LibreSvipError::msg(format!(
                    "Notes overlapped near bar {bar_index}"
                )));
            } else if portamento.vocaloid_mode
                && next_note.start_pos - current_note.end_pos()
                    >= MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS
            {
                max_portamento_time = 0.0;
            }
            let next_head = synchronizer.get_actual_secs_from_ticks(next_note.start_pos);
            let next_dur = synchronizer
                .get_duration_secs_from_ticks(next_note.start_pos, next_note.end_pos());
            if portamento.vocaloid_mode && next_note.start_pos > current_note.end_pos() {
                let current_tail = synchronizer.get_actual_secs_from_ticks(current_note.end_pos());
                if prev_portamento_end < current_tail {
                    interval_dict.set(
                        prev_portamento_end,
                        current_tail,
                        IntervalValue::Constant(current_note.key_number as f64),
                    );
                }
                current_note = next_note;
                current_head = next_head;
                current_dur = next_dur;
                let (ticks, time) = vocaloid_max_portamento(current_note);
                max_portamento_ticks = ticks;
                max_portamento_time = time;
                current_portamento =
                    (current_dur * max_portamento_percent).min(max_portamento_time);
                prev_portamento_end = current_head;
                continue;
            }
            let next_portamento = (next_dur * max_portamento_percent).min(max_portamento_time);
            let (interval, middle_time) = if portamento.vocaloid_mode {
                let middle_position = if next_note.lyric == "-" {
                    current_note.end_pos() as f64
                } else {
                    (next_note.start_pos + current_note.end_pos()) as f64 / 2.0
                } - max_portamento_ticks * 0.4;
                let interval = synchronizer.get_duration_secs_from_ticks(
                    (middle_position - max_portamento_ticks) as i64,
                    middle_position as i64,
                );
                let middle_time =
                    synchronizer.get_actual_secs_from_ticks(middle_position as i64);
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
            interval_dict.set(
                prev_portamento_end,
                current_portamento_start,
                IntervalValue::Constant(current_note.key_number as f64),
            );
            if current_note.key_number == next_note.key_number {
                interval_dict.set(
                    current_portamento_start,
                    current_portamento_end,
                    IntervalValue::Constant(current_note.key_number as f64),
                );
            } else if current_portamento_start < current_portamento_end {
                interval_dict.set(
                    current_portamento_start,
                    current_portamento_end,
                    IntervalValue::Linear {
                        start: current_portamento_start,
                        start_value: current_note.key_number as f64,
                        end: current_portamento_end,
                        end_value: next_note.key_number as f64,
                        vocaloid: portamento.vocaloid_mode,
                    },
                );
            }
            current_note = next_note;
            current_head = next_head;
            current_dur = next_dur;
            current_portamento = next_portamento;
            prev_portamento_end = current_portamento_end;
            if portamento.vocaloid_mode {
                let (ticks, time) = vocaloid_max_portamento(current_note);
                max_portamento_ticks = ticks;
                max_portamento_time = time;
            }
        }
        interval_dict.set(
            prev_portamento_end,
            f64::INFINITY,
            IntervalValue::Constant(current_note.key_number as f64),
        );
        Ok(Self {
            synchronizer: synchronizer.clone(),
            interval_dict,
        })
    }

    pub fn pitch_at_ticks(&self, ticks: i64) -> Option<f64> {
        self.pitch_at_secs(self.synchronizer.get_actual_secs_from_ticks(ticks))
    }

    pub fn pitch_at_ticks_batch(&self, ticks_list: &[i64]) -> Vec<Option<f64>> {
        // The Python fast path (ordered lookup) only avoids recomputing
        // seconds; per-point results are identical.
        ticks_list
            .iter()
            .map(|ticks| self.pitch_at_ticks(*ticks))
            .collect()
    }

    fn pitch_at_secs(&self, secs: f64) -> Option<f64> {
        let value = self.interval_dict.get(secs)?;
        if !value.is_truthy() {
            return None;
        }
        Some(value.evaluate(secs) * 100.0)
    }
}

// ---------------------------------------------------------------------------
// RelativePitchCurve
// ---------------------------------------------------------------------------

/// `libresvip.model.relative_pitch_curve.RelativePitchCurve` with the default
/// field values used by the `mid` and `ufdata` plugins.
#[derive(Clone, Debug)]
pub struct RelativePitchCurve {
    pub first_bar_length: i64,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub pitch_interval: i64,
    pub is_staircase: bool,
}

impl RelativePitchCurve {
    pub fn new(first_bar_length: i64) -> Self {
        Self {
            first_bar_length,
            lower_bound: 0.0,
            upper_bound: f64::INFINITY,
            pitch_interval: 5,
            is_staircase: true,
        }
    }

    /// `to_absolute`.
    pub fn to_absolute(
        &self,
        points: &[Point],
        pitch_simulator: &PitchSimulator,
    ) -> Result<ParamCurve> {
        Ok(ParamCurve::new(
            self.convert_relativity(points, pitch_simulator, true),
        ))
    }

    /// `from_absolute`.
    pub fn from_absolute(
        &self,
        points: &[Point],
        pitch_simulator: &PitchSimulator,
    ) -> Vec<Point> {
        self.convert_relativity(points, pitch_simulator, false)
    }

    /// `_convert_relativity`.
    fn convert_relativity(
        &self,
        points: &[Point],
        pitch_simulator: &PitchSimulator,
        to_absolute: bool,
    ) -> Vec<Point> {
        let first_bar_length = self.first_bar_length;
        let pitch_offset = if to_absolute { 0 } else { -first_bar_length };
        let converted_offset = if to_absolute {
            first_bar_length
        } else {
            -first_bar_length
        };
        let point_tick_positions: Vec<i64> =
            points.iter().map(|point| point.x + pitch_offset).collect();
        let pitch_values = pitch_simulator.pitch_at_ticks_batch(&point_tick_positions);
        let mut pitch_value_map: HashMap<i64, Option<f64>> = point_tick_positions
            .iter()
            .copied()
            .zip(pitch_values.iter().copied())
            .collect();

        let mut converted_data: Vec<Point> = Vec::new();
        let mut prev_x: Option<i64> = None;
        let mut prev_y: Option<f64> = None;
        for point in points {
            let position = point.x + pitch_offset;
            let cur_x = point.x + converted_offset;
            let base_key = match pitch_value_map.get(&position) {
                Some(value) => *value,
                None => {
                    let value = pitch_simulator.pitch_at_ticks(position);
                    pitch_value_map.insert(position, value);
                    value
                }
            };
            let (y, rel_y) = if base_key.is_none() {
                (None, None)
            } else if !to_absolute && point.y == -100 {
                if point.x == START_POINT_X || point.x == END_POINT_X {
                    continue;
                }
                (Some(0.0), Some(0.0))
            } else {
                let base_key = base_key.unwrap_or_default();
                let rel_y = if to_absolute {
                    point.y as f64
                } else {
                    point.y as f64 - base_key
                };
                let y = if to_absolute { rel_y + base_key } else { rel_y };
                (Some(y), Some(rel_y))
            };
            if let (Some(y), Some(prev_x_value), Some(prev_y_value)) = (y, prev_x, prev_y) {
                if !converted_data.is_empty() && cur_x - prev_x_value > self.pitch_interval {
                    let mut tick = prev_x_value + self.pitch_interval;
                    while tick < cur_x {
                        let key_position = if to_absolute {
                            tick - first_bar_length
                        } else {
                            tick
                        };
                        let tick_key = match pitch_value_map.get(&key_position) {
                            Some(value) => *value,
                            None => {
                                let value = pitch_simulator.pitch_at_ticks(key_position);
                                pitch_value_map.insert(key_position, value);
                                value
                            }
                        };
                        let Some(tick_key) = tick_key else { break };
                        if to_absolute {
                            let interpolated_rel_y = if self.is_staircase || rel_y.is_none() {
                                prev_y_value
                            } else {
                                prev_y_value
                                    + (rel_y.unwrap_or_default() - prev_y_value)
                                        * (tick - prev_x_value) as f64
                                        / (cur_x - prev_x_value) as f64
                            };
                            converted_data.push(Point::new(
                                tick,
                                round_to_i64(interpolated_rel_y + tick_key),
                            ));
                        } else if !(self.is_staircase && prev_y_value == y) {
                            converted_data.push(Point::new(
                                tick,
                                round_to_i64(
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
                converted_data.push(Point::new(cur_x, round_to_i64(y)));
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
            if self.upper_bound == f64::INFINITY {
                let last = *converted_data
                    .last()
                    .expect("converted_data is not empty");
                converted_data.push(Point::new(last.x, -100));
                converted_data.push(Point::end_point());
            }
        }
        converted_data
    }
}

// ---------------------------------------------------------------------------
// MIDI pitch data (`midi_pitch.py`)
// ---------------------------------------------------------------------------

/// `midi_pitch.ControlEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlEvent {
    pub tick: i64,
    pub value: i64,
}

/// `midi_pitch.MIDIPitchData`.
#[derive(Clone, Debug, Default)]
pub struct MidiPitchData {
    pub pit: Vec<ControlEvent>,
    pub pbs: Vec<ControlEvent>,
}

/// Port of `midi_pitch.generate_for_midi`.
pub fn generate_for_midi(
    first_bar_length: i64,
    pitch: &ParamCurve,
    notes: &[Note],
    time_signature_list: &[TimeSignature],
    synchronizer: &TimeSynchronizer,
) -> Result<Option<MidiPitchData>> {
    let pitch_simulator = PitchSimulator::new(
        synchronizer,
        Portamento::no_portamento(),
        notes,
        time_signature_list,
    )?;
    let data = RelativePitchCurve::new(first_bar_length).from_absolute(&pitch.points, &pitch_simulator);
    if data.is_empty() {
        return Ok(None);
    }
    let mut pitch_sectioned: Vec<Vec<Point>> = vec![Vec::new()];
    let mut current_pos = 0i64;
    for pitch_event in data {
        let last = pitch_sectioned.last_mut().expect("one section");
        if last.is_empty()
            || pitch_event.x - current_pos < MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS
        {
            last.push(pitch_event);
        } else {
            pitch_sectioned.push(vec![pitch_event]);
        }
        current_pos = pitch_event.x;
    }
    let mut pit: Vec<ControlEvent> = Vec::new();
    let mut pbs: Vec<ControlEvent> = Vec::new();
    for section in &pitch_sectioned {
        if section.is_empty() {
            continue;
        }
        let max_abs_value = section
            .iter()
            .map(|point| (point.y as f64 / 100.0).abs())
            .fold(0.0f64, f64::max);
        let mut pbs_for_this_section =
            (max_abs_value.ceil() as i64).min(MAX_PITCH_BEND_SENSITIVITY);
        if pbs_for_this_section > DEFAULT_PITCH_BEND_SENSITIVITY {
            let last = section.last().expect("non-empty section");
            pbs.push(ControlEvent {
                tick: last.x + MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS / 2,
                value: DEFAULT_PITCH_BEND_SENSITIVITY,
            });
        } else {
            pbs_for_this_section = DEFAULT_PITCH_BEND_SENSITIVITY;
        }
        for point in section {
            let pitch_value = point.y as f64;
            let value = clamp(
                pitch_value
                    * (if pitch_value > 0.0 {
                        PITCH_MAX_VALUE as f64
                    } else {
                        (PITCH_MAX_VALUE + 1) as f64
                    })
                    / 100.0
                    / pbs_for_this_section as f64,
                (-PITCH_MAX_VALUE - 1) as f64,
                PITCH_MAX_VALUE as f64,
            );
            pit.push(ControlEvent {
                tick: point.x,
                value: value as i64,
            });
        }
    }
    Ok(Some(MidiPitchData { pit, pbs }))
}
