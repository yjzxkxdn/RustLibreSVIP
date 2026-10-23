//! Port of `libresvip.model.techno_speech_pitch` (the part used by VoiSona).
//!
//! The pitch data stored in a VoiSona Mobile project is a list of "log F0"
//! points expressed in time units: an event with `Index`, optional `Repeat` and
//! a `Value` which is `ln(Hz)`. Conversion to the internal model needs the
//! tempo map, the first bar length (`tick_prefix`) and the optional vibrato
//! curves.

use std::f64::consts::{E, TAU};

use libresvip_core::core::compat::find_last_index;
use libresvip_core::core::tick_counter::shift_tempo_list;
use libresvip_core::core::warning_types::show_warning;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{ParamCurve, SongTempo};
use libresvip_core::model::point::Point;

use super::constants::{MIN_DATA_LENGTH, TEMP_VALUE_AS_NULL, TIME_UNIT_AS_TICKS_PER_BPM};

/// A decoded VoiSona parameter event (`TechnoSpeechParamEvent`).
#[derive(Clone, Debug, PartialEq)]
pub struct ParamEvent {
    pub idx: Option<i64>,
    pub repeat: Option<i64>,
    pub value: f64,
}

/// Internal event with all fields as floats (`TechnoSpeechParamEventFloat`).
#[derive(Clone, Debug, PartialEq)]
struct FloatEvent {
    idx: Option<f64>,
    repeat: Option<f64>,
    value: Option<f64>,
}

/// `TechnoSpeechTrackPitchData`.
#[derive(Clone, Debug, Default)]
pub struct TrackPitchData {
    pub events: Vec<ParamEvent>,
    pub tempos: Vec<SongTempo>,
    pub tick_prefix: i64,
    pub vibrato_amplitude_events: Vec<ParamEvent>,
    pub vibrato_frequency_events: Vec<ParamEvent>,
}

impl TrackPitchData {
    /// `TechnoSpeechTrackPitchData.length`.
    pub fn length(&self) -> i64 {
        if self.events.is_empty() {
            return MIN_DATA_LENGTH;
        }
        let last_has_index = find_last_index(&self.events, |event| event.idx.is_some());
        if last_has_index < 0 {
            return MIN_DATA_LENGTH;
        }
        let start = last_has_index as usize;
        let mut length = self.events[start].idx.unwrap_or(0);
        for event in &self.events[start..] {
            length += repeat_or_one_i64(event.repeat);
        }
        length + MIN_DATA_LENGTH
    }
}

#[inline]
fn repeat_or_one_i64(value: Option<i64>) -> i64 {
    match value {
        Some(value) if value != 0 => value,
        _ => 1,
    }
}

#[inline]
fn repeat_or_one_f64(value: Option<f64>) -> f64 {
    match value {
        Some(value) if value != 0.0 => value,
        _ => 1.0,
    }
}

#[inline]
fn midi2hz(midi: f64) -> f64 {
    440.0 * 2.0f64.powf((midi - 69.0) / 12.0)
}

#[inline]
fn hz2midi(hz: f64) -> f64 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

// ---------------------------------------------------------------------------
// Local time synchroniser that accepts fractional ticks
// ---------------------------------------------------------------------------

/// `TimeSynchronizer._secs_at_tick`, but with a float tick argument: the pitch
/// code passes normalized (fractional) ticks to `get_actual_secs_from_ticks`.
struct TickSync {
    positions: Vec<i64>,
    bpms: Vec<f64>,
    cum_secs: Vec<f64>,
}

impl TickSync {
    fn new(tempos: &[SongTempo]) -> Self {
        let positions: Vec<i64> = tempos.iter().map(|tempo| tempo.position).collect();
        let bpms: Vec<f64> = tempos.iter().map(|tempo| tempo.bpm).collect();
        let mut cum_secs = vec![0.0f64; tempos.len()];
        for index in 1..tempos.len() {
            let dt = positions[index] - positions[index - 1];
            cum_secs[index] = cum_secs[index - 1] + dt as f64 / bpms[index - 1] / 8.0;
        }
        Self {
            positions,
            bpms,
            cum_secs,
        }
    }

    fn secs_at_tick(&self, ticks: f64) -> Result<f64> {
        if self.positions.is_empty() {
            return Err(LibreSvipError::msg("no tempo found"));
        }
        let index = self
            .positions
            .partition_point(|&position| (position as f64) <= ticks)
            .saturating_sub(1);
        Ok(self.cum_secs[index] + (ticks - self.positions[index] as f64) / self.bpms[index] / 8.0)
    }

    /// `TimeSynchronizer.get_actual_secs_from_ticks` — it is defined as
    /// `get_duration_secs_from_ticks(0, ticks)`, so a first tempo that does not
    /// sit at tick 0 shifts every result.
    fn secs_from_ticks_f64(&self, ticks: f64) -> Result<f64> {
        Ok(self.secs_at_tick(ticks)? - self.secs_at_tick(0.0)?)
    }

    fn secs_from_ticks(&self, ticks: i64) -> Result<f64> {
        self.secs_from_ticks_f64(ticks as f64)
    }

    fn duration_secs(&self, start_ticks: i64, end_ticks: i64) -> Result<f64> {
        Ok(self.secs_at_tick(end_ticks as f64)? - self.secs_at_tick(start_ticks as f64)?)
    }
}

// ---------------------------------------------------------------------------
// Interval dictionaries (port of the `PiecewiseIntervalDict` usage)
// ---------------------------------------------------------------------------

/// `portion.closedopen(lower, upper)` entries with overwrite semantics.
#[derive(Clone, Debug)]
struct RangeMap<T> {
    entries: Vec<(f64, f64, T)>,
}

impl<T> Default for RangeMap<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T: Clone> RangeMap<T> {
    fn set(&mut self, lower: f64, upper: f64, value: T) {
        if !(lower < upper) {
            return;
        }
        let mut remaining: Vec<(f64, f64, T)> = Vec::with_capacity(self.entries.len() + 1);
        for (entry_lower, entry_upper, entry_value) in self.entries.drain(..) {
            let overlaps = entry_lower < upper && lower < entry_upper;
            if !overlaps {
                remaining.push((entry_lower, entry_upper, entry_value));
                continue;
            }
            if entry_lower < lower {
                remaining.push((entry_lower, lower, entry_value.clone()));
            }
            if upper < entry_upper {
                remaining.push((upper, entry_upper, entry_value));
            }
        }
        remaining.push((lower, upper, value));
        remaining.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.entries = remaining;
    }

    fn remove(&mut self, lower: f64, upper: f64) {
        if !(lower < upper) {
            return;
        }
        let mut remaining: Vec<(f64, f64, T)> = Vec::with_capacity(self.entries.len());
        for (entry_lower, entry_upper, entry_value) in self.entries.drain(..) {
            let overlaps = entry_lower < upper && lower < entry_upper;
            if !overlaps {
                remaining.push((entry_lower, entry_upper, entry_value));
                continue;
            }
            if entry_lower < lower {
                remaining.push((entry_lower, lower, entry_value.clone()));
            }
            if upper < entry_upper {
                remaining.push((upper, entry_upper, entry_value));
            }
        }
        self.entries = remaining;
    }

    fn find(&self, x: f64) -> Option<&T> {
        self.entries
            .iter()
            .find(|(lower, upper, _)| *lower <= x && x < *upper)
            .map(|(_, _, value)| value)
    }
}

/// Interval map used for the vibrato amplitude: `None` cells behave as absent
/// (Python's `_get_func` cannot tell a stored `None` from a miss).
#[derive(Clone, Debug, Default)]
struct ParamMap {
    inner: RangeMap<f64>,
}

impl ParamMap {
    fn get(&self, x: f64) -> Option<f64> {
        self.inner.find(x).copied()
    }

    fn set(&mut self, lower: f64, upper: f64, value: Option<f64>) {
        match value {
            Some(value) => self.inner.set(lower, upper, value),
            None => self.inner.remove(lower, upper),
        }
    }
}

/// A stored vibrato waveform sample.
#[derive(Clone, Debug)]
enum WaveValue {
    Constant(f64),
    Wave { shift: f64, omega: f64, phase: f64 },
}

#[derive(Clone, Debug, Default)]
struct WaveMap {
    inner: RangeMap<WaveValue>,
}

impl WaveMap {
    fn get(&self, x: f64) -> Option<f64> {
        self.inner.find(x).map(|value| match value {
            WaveValue::Constant(value) => *value,
            WaveValue::Wave {
                shift,
                omega,
                phase,
            } => vibrato_curve(x, *shift, *omega, *phase),
        })
    }

    fn set(&mut self, lower: f64, upper: f64, value: WaveValue) {
        self.inner.set(lower, upper, value);
    }
}

#[inline]
fn vibrato_curve(value: f64, shift: f64, omega: f64, phase: f64) -> f64 {
    (omega * (value - shift) + phase).sin()
}

// ---------------------------------------------------------------------------
// Reading a project pitch curve
// ---------------------------------------------------------------------------

/// `pitch_from_techno_speech_track`.
pub fn pitch_from_track(data: &TrackPitchData) -> Result<Option<ParamCurve>> {
    let mut converted_points = vec![Point::start_point()];
    let mut current_value: i64 = -100;

    let synchronizer = TickSync::new(&data.tempos);
    let expanded_tempos = expand(&data.tempos, data.tick_prefix);
    let vibrato_amplitude_interval_dict = build_param_interval_dict(
        &data.vibrato_amplitude_events,
        &synchronizer,
        data.tick_prefix,
        &expanded_tempos,
    )?;
    let vibrato_value_interval_dict = build_wave_interval_dict(
        &data.vibrato_frequency_events,
        &synchronizer,
        data.tick_prefix,
        &expanded_tempos,
    )?;
    let ending_points = append_ending_points(&data.events);
    let events_normalized = shape_events(normalize_to_tick(
        &ending_points,
        data.tick_prefix,
        &expanded_tempos,
    )?);

    let mut next_pos: Option<f64> = None;
    for event in &events_normalized {
        let Some(event_idx) = event.idx else {
            continue;
        };
        let pos = event_idx as i64 - data.tick_prefix;
        let secs = synchronizer.secs_from_ticks(pos)?;
        let length = event.repeat.unwrap_or(1.0);
        let mut overflow = false;
        let value = match event.value {
            None => -100,
            Some(raw) => {
                let hz = E.powf(raw);
                if hz.is_finite() {
                    if hz == 0.0 {
                        return Err(LibreSvipError::msg("Pitch value is out of bounds"));
                    }
                    let scaled = hz2midi(hz) * 100.0;
                    if scaled.is_finite() {
                        scaled.round_ties_even() as i64
                    } else {
                        overflow = true;
                        0
                    }
                } else {
                    overflow = true;
                    0
                }
            }
        };
        if overflow {
            show_warning("Pitch value is out of bounds");
        } else {
            if value != current_value || next_pos != Some(pos as f64) {
                converted_points.push(Point::new(pos, value));
                if value == -100 {
                    converted_points.push(Point::new(pos, value));
                }
                current_value = value;
            }
            let secs_step = synchronizer.duration_secs(pos, pos + 5)?;
            let end = (pos as f64 + length) as i64;
            let mut pos_x = pos;
            let mut secs = secs;
            while pos_x < end {
                match vibrato_value_interval_dict.get(secs) {
                    Some(value_diff) if value_diff != 0.0 => {
                        let amplitude = vibrato_amplitude_interval_dict.get(secs).unwrap_or(1.0);
                        converted_points.push(Point::new(
                            pos_x,
                            (value as f64 + value_diff * amplitude).round_ties_even() as i64,
                        ));
                    }
                    _ => break,
                }
                secs += secs_step;
                pos_x += 5;
            }
        }
        next_pos = Some(pos as f64 + length);
    }
    converted_points.push(Point::end_point());

    Ok(if converted_points.len() > 2 {
        Some(ParamCurve::new(converted_points))
    } else {
        None
    })
}

/// `append_ending_points`.
fn append_ending_points(events: &[ParamEvent]) -> Vec<FloatEvent> {
    let mut result = Vec::new();
    let mut next_pos: Option<f64> = None;
    for event in events {
        let pos = match event.idx {
            Some(idx) => Some(idx as f64),
            None => next_pos,
        };
        let Some(pos) = pos else {
            continue;
        };
        let length = match event.repeat {
            Some(repeat) => repeat as f64,
            None => 1.0,
        };
        if let Some(next) = next_pos {
            if next < pos {
                result.push(FloatEvent {
                    idx: Some(next),
                    repeat: None,
                    value: Some(TEMP_VALUE_AS_NULL),
                });
            }
        }
        result.push(FloatEvent {
            idx: Some(pos),
            repeat: Some(length),
            value: Some(event.value),
        });
        next_pos = Some(pos + length);
    }
    if let Some(next) = next_pos {
        result.push(FloatEvent {
            idx: Some(next),
            repeat: None,
            value: Some(TEMP_VALUE_AS_NULL),
        });
    }
    result
}

/// `normalize_to_tick`.
fn normalize_to_tick(
    events: &[FloatEvent],
    tick_prefix: i64,
    tempos: &[(i64, i64, f64)],
) -> Result<Vec<FloatEvent>> {
    if events.is_empty() {
        return Ok(Vec::new());
    }
    if tempos.is_empty() {
        return Err(LibreSvipError::msg("no tempo found"));
    }
    let mut events_normalized = Vec::with_capacity(events.len());
    let mut current_tempo_index = 0usize;
    let mut next_pos = 0.0f64;
    let mut next_tick_pos = 0.0f64;
    for event in events {
        let pos = event.idx.unwrap_or(next_pos);
        let tick_pos = match event.idx {
            None => next_tick_pos,
            Some(idx) => {
                while current_tempo_index + 1 < tempos.len()
                    && (tempos[current_tempo_index + 1].0 as f64) <= idx
                {
                    current_tempo_index += 1;
                }
                let ticks_in_time_unit =
                    TIME_UNIT_AS_TICKS_PER_BPM * tempos[current_tempo_index].2;
                tempos[current_tempo_index].1 as f64
                    + (idx - tempos[current_tempo_index].0 as f64) * ticks_in_time_unit
            }
        };
        let repeat = event.repeat.unwrap_or(1.0);
        let mut remaining_repeat = repeat;
        let mut repeat_in_ticks = 0.0f64;
        while current_tempo_index + 1 < tempos.len()
            && (tempos[current_tempo_index + 1].0 as f64) < pos + repeat
        {
            repeat_in_ticks += tempos[current_tempo_index + 1].1 as f64
                - tick_pos.max(tempos[current_tempo_index].1 as f64);
            remaining_repeat -= tempos[current_tempo_index + 1].0 as f64
                - pos.max(tempos[current_tempo_index].0 as f64);
            current_tempo_index += 1;
        }
        repeat_in_ticks +=
            remaining_repeat * TIME_UNIT_AS_TICKS_PER_BPM * tempos[current_tempo_index].2;
        next_pos = pos + repeat;
        next_tick_pos = tick_pos + repeat_in_ticks;
        events_normalized.push(FloatEvent {
            idx: Some(tick_pos),
            repeat: Some(repeat_in_ticks),
            value: event.value,
        });
    }
    Ok(events_normalized
        .into_iter()
        .map(|tick| FloatEvent {
            idx: tick.idx.map(|idx| idx + tick_prefix as f64),
            repeat: tick.repeat,
            value: match tick.value {
                Some(value) if value == TEMP_VALUE_AS_NULL => None,
                other => other,
            },
        })
        .collect())
}

/// `shape_events`.
fn shape_events(events_with_full_params: Vec<FloatEvent>) -> Vec<FloatEvent> {
    let mut result: Vec<FloatEvent> = Vec::new();
    for event in events_with_full_params {
        if let Some(repeat) = event.repeat {
            if repeat > 0.0 {
                if let Some(last) = result.last_mut() {
                    if last.idx == event.idx {
                        *last = event;
                    } else {
                        result.push(event);
                    }
                } else {
                    result.push(event);
                }
            }
        }
    }
    result
}

/// `expand` — map tempo positions (ticks) to positions in time units.
pub fn expand(tempos: &[SongTempo], tick_prefix: i64) -> Vec<(i64, i64, f64)> {
    let mut result: Vec<(i64, i64, f64)> = Vec::with_capacity(tempos.len());
    for (index, tempo) in tempos.iter().enumerate() {
        if index == 0 {
            result.push((0, tick_prefix, tempo.bpm));
        } else {
            let (last_pos, last_tick_pos, last_bpm) = result[result.len() - 1];
            let ticks_in_time_unit = TIME_UNIT_AS_TICKS_PER_BPM * last_bpm;
            let new_pos =
                last_pos as f64 + (tempo.position - last_tick_pos) as f64 / ticks_in_time_unit;
            result.push((new_pos as i64, tempo.position, tempo.bpm));
        }
    }
    result
}

/// `_normalized_continuous_parts`.
fn normalized_continuous_parts(
    events: &[ParamEvent],
    tick_prefix: i64,
    tempos: &[(i64, i64, f64)],
) -> Result<Vec<Vec<FloatEvent>>> {
    let mut parts: Vec<Vec<ParamEvent>> = Vec::new();
    for event in events {
        let start_new = event.idx.is_some();
        match parts.last_mut() {
            Some(part) if !(start_new && !part.is_empty()) => part.push(event.clone()),
            _ => parts.push(vec![event.clone()]),
        }
    }
    let mut result = Vec::with_capacity(parts.len());
    for part in parts {
        let with_end = append_ending_points(&part);
        result.push(normalize_to_tick(&with_end, tick_prefix, tempos)?);
    }
    Ok(result)
}

/// `build_param_interval_dict`.
fn build_param_interval_dict(
    events: &[ParamEvent],
    synchronizer: &TickSync,
    tick_prefix: i64,
    expanded_tempos: &[(i64, i64, f64)],
) -> Result<ParamMap> {
    let mut param_interval_dict = ParamMap::default();
    for continuous_part in normalized_continuous_parts(events, tick_prefix, expanded_tempos)? {
        let mut previous: Option<FloatEvent> = None;
        for next_event in continuous_part {
            let Some(next_idx) = next_event.idx else {
                continue;
            };
            let next_start = synchronizer.secs_from_ticks_f64(next_idx - tick_prefix as f64)?;
            if let Some(prev) = &previous {
                if let Some(prev_idx) = prev.idx {
                    let prev_end = synchronizer.secs_from_ticks_f64(
                        prev_idx + repeat_or_one_f64(prev.repeat) - tick_prefix as f64,
                    )?;
                    if prev_end != 0.0 && prev_end < next_start {
                        param_interval_dict.set(prev_end, next_start, next_event.value);
                    }
                }
            }
            let next_end = synchronizer.secs_from_ticks_f64(
                next_idx + repeat_or_one_f64(next_event.repeat) - tick_prefix as f64,
            )?;
            param_interval_dict.set(next_start, next_end, next_event.value);
            previous = Some(next_event);
        }
    }
    Ok(param_interval_dict)
}

/// `build_wave_interval_dict`.
fn build_wave_interval_dict(
    events: &[ParamEvent],
    synchronizer: &TickSync,
    tick_prefix: i64,
    expanded_tempos: &[(i64, i64, f64)],
) -> Result<WaveMap> {
    let mut param_interval_dict = WaveMap::default();
    let mut omega = TAU * 6.0;
    for continuous_part in normalized_continuous_parts(events, tick_prefix, expanded_tempos)? {
        let mut phase = 0.0f64;
        let mut previous: Option<FloatEvent> = None;
        for next_event in continuous_part {
            let Some(next_idx) = next_event.idx else {
                continue;
            };
            let next_start = synchronizer.secs_from_ticks_f64(next_idx - tick_prefix as f64)?;
            let next_end = synchronizer.secs_from_ticks_f64(
                next_idx + repeat_or_one_f64(next_event.repeat) - tick_prefix as f64,
            )?;
            if let Some(prev) = &previous {
                if let Some(prev_idx) = prev.idx {
                    let prev_end = synchronizer.secs_from_ticks_f64(
                        prev_idx + repeat_or_one_f64(prev.repeat) - tick_prefix as f64,
                    )?;
                    if prev_end != 0.0 && prev_end < next_start {
                        let prev_start = synchronizer.secs_from_ticks_f64(prev_idx - tick_prefix as f64)?;
                        param_interval_dict.set(
                            prev_end,
                            next_start,
                            WaveValue::Constant(vibrato_curve(prev_end, prev_start, omega, phase)),
                        );
                    }
                }
            }
            omega = TAU
                * match next_event.value {
                    Some(value) if value != 0.0 => value,
                    _ => 6.0,
                };
            param_interval_dict.set(
                next_start,
                next_end,
                WaveValue::Wave {
                    shift: next_start,
                    omega,
                    phase,
                },
            );
            phase += (next_end - next_start) * omega;
            previous = Some(next_event);
        }
    }
    Ok(param_interval_dict)
}

// ---------------------------------------------------------------------------
// Writing a project pitch curve
// ---------------------------------------------------------------------------

/// `generate_for_techno_speech`.
pub fn generate_for_techno_speech(
    pitch: &ParamCurve,
    tempos: &[SongTempo],
    tick_prefix: i64,
) -> Result<Option<TrackPitchData>> {
    let mut events_with_full_params: Vec<FloatEvent> = Vec::new();
    let points = &pitch.points;
    for (index, this_point) in points.iter().enumerate() {
        let next_point = points.get(index + 1);
        let end_tick = next_point.map(|next| next.x - tick_prefix);
        let point_index = this_point.x - tick_prefix;
        let repeat = match end_tick {
            Some(end_tick) if end_tick != 0 => (end_tick - point_index) as f64,
            _ => 1.0,
        };
        let repeat = repeat.max(1.0);
        let value = if this_point.y != -100 {
            Some(midi2hz(this_point.y as f64 / 100.0).ln())
        } else {
            None
        };
        if let Some(value) = value {
            let keep = match next_point {
                Some(next) => next.y != -100,
                None => true,
            };
            if keep {
                events_with_full_params.push(FloatEvent {
                    idx: Some(point_index as f64),
                    repeat: Some(repeat),
                    value: Some(value),
                });
            }
        }
    }
    let are_events_connected_to_next: Vec<bool> = events_with_full_params
        .iter()
        .enumerate()
        .map(|(index, this_event)| match events_with_full_params.get(index + 1) {
            Some(next_event) => {
                this_event.idx.unwrap_or(0.0) + this_event.repeat.unwrap_or(0.0)
                    >= next_event.idx.unwrap_or(0.0)
            }
            None => false,
        })
        .collect();
    let mut events = denormalize_from_tick(&events_with_full_params, tempos, tick_prefix)?;
    events = restore_connection(&events, &are_events_connected_to_next);
    events = merge_events_if_possible(events);
    events = remove_redundant_index(events);
    events = remove_redundant_repeat(events);
    if events.is_empty() {
        return Ok(None);
    }
    Ok(Some(TrackPitchData {
        events,
        tempos: Vec::new(),
        tick_prefix,
        vibrato_amplitude_events: Vec::new(),
        vibrato_frequency_events: Vec::new(),
    }))
}

/// `denormalize_from_tick`.
fn denormalize_from_tick(
    events_with_full_params: &[FloatEvent],
    tempos_in_ticks: &[SongTempo],
    tick_prefix: i64,
) -> Result<Vec<ParamEvent>> {
    let shifted = shift_tempo_list(tempos_in_ticks, tick_prefix);
    let tempos = expand(&shifted, tick_prefix);
    let events_with_full_params: Vec<FloatEvent> = events_with_full_params
        .iter()
        .map(|event| FloatEvent {
            idx: event.idx.map(|idx| idx + tick_prefix as f64),
            repeat: event.repeat,
            value: event.value,
        })
        .collect();
    let mut events = Vec::new();
    let mut current_tempo_index = 0usize;
    let mut tick_pos: Option<f64> = None;
    for event_double in &events_with_full_params {
        if let Some(idx) = event_double.idx {
            tick_pos = Some(idx);
        }
        let (current_tick_pos, idx) = match (tick_pos, event_double.idx) {
            (Some(current_tick_pos), Some(idx)) => (current_tick_pos, idx),
            _ => return Err(LibreSvipError::msg("Invalid event")),
        };
        while current_tempo_index + 1 < tempos.len()
            && (tempos[current_tempo_index + 1].1 as f64) < current_tick_pos
        {
            current_tempo_index += 1;
        }
        if tempos.is_empty() {
            return Err(LibreSvipError::msg("no tempo found"));
        }
        let ticks_per_time_unit = tempos[current_tempo_index].2 * TIME_UNIT_AS_TICKS_PER_BPM;
        let pos = tempos[current_tempo_index].0 as f64
            + (idx - tempos[current_tempo_index].1 as f64) / ticks_per_time_unit;
        let mut repeat_in_ticks = event_double.repeat.unwrap_or(1.0);
        let mut repeat = 0.0f64;
        while (current_tempo_index + 1 < tempos.len())
            && ((tempos[current_tempo_index + 1].1 as f64)
                < current_tick_pos + repeat_in_ticks)
        {
            repeat += tempos[current_tempo_index + 1].0 as f64
                - pos.max(tempos[current_tempo_index].0 as f64);
            repeat_in_ticks -= tempos[current_tempo_index + 1].1 as f64
                - current_tick_pos.max(tempos[current_tempo_index].1 as f64);
            current_tempo_index += 1;
        }
        repeat += repeat_in_ticks / (TIME_UNIT_AS_TICKS_PER_BPM * tempos[current_tempo_index].2);
        events.push(ParamEvent {
            idx: Some(pos.round_ties_even() as i64),
            repeat: Some(repeat.max(1.0).round_ties_even() as i64),
            value: event_double.value.unwrap_or(0.0),
        });
    }
    Ok(events)
}

/// `restore_connection`.
fn restore_connection(
    events: &[ParamEvent],
    are_events_connected_to_next: &[bool],
) -> Vec<ParamEvent> {
    let mut new_events = Vec::with_capacity(events.len());
    for (index, prev_event) in events.iter().enumerate() {
        match events.get(index + 1) {
            Some(next_event)
                if are_events_connected_to_next
                    .get(index)
                    .copied()
                    .unwrap_or(false) =>
            {
                new_events.push(ParamEvent {
                    idx: prev_event.idx,
                    repeat: Some(next_event.idx.unwrap_or(0) - prev_event.idx.unwrap_or(0)),
                    value: prev_event.value,
                });
            }
            _ => new_events.push(prev_event.clone()),
        }
    }
    new_events
}

/// `merge_events_if_possible`.
fn merge_events_if_possible(events: Vec<ParamEvent>) -> Vec<ParamEvent> {
    let mut new_events: Vec<ParamEvent> = Vec::new();
    for mut event in events {
        if new_events.is_empty() {
            new_events.push(event);
            continue;
        }
        let mut last_event = new_events[new_events.len() - 1].clone();
        let overlapped_len = last_event.idx.unwrap_or(0) + last_event.repeat.unwrap_or(0)
            - event.idx.unwrap_or(0);
        if overlapped_len > 0 {
            if let Some(last) = new_events.last_mut() {
                last.repeat = Some(event.idx.unwrap_or(0) - last_event.idx.unwrap_or(0));
            }
            event = ParamEvent {
                idx: Some(overlapped_len + event.idx.unwrap_or(0)),
                repeat: Some(event.repeat.unwrap_or(0) - overlapped_len),
                value: event.value,
            };
            last_event = ParamEvent {
                idx: event.idx,
                repeat: Some(overlapped_len),
                value: event.value + last_event.value,
            };
            new_events.push(last_event.clone());
        }
        if last_event.value == event.value
            && last_event.idx.unwrap_or(0) + last_event.repeat.unwrap_or(0)
                == event.idx.unwrap_or(0)
        {
            if let Some(last) = new_events.last_mut() {
                last.repeat = Some(last_event.repeat.unwrap_or(0) + event.repeat.unwrap_or(0));
            }
        } else {
            new_events.push(event);
        }
    }
    new_events
}

/// `remove_redundant_index`.
fn remove_redundant_index(events: Vec<ParamEvent>) -> Vec<ParamEvent> {
    let mut new_events: Vec<ParamEvent> = Vec::new();
    for event in events {
        if new_events.is_empty() {
            new_events.push(event);
        } else {
            let prev_event = new_events[new_events.len() - 1].clone();
            if prev_event.idx.is_some()
                && prev_event.repeat.is_some()
                && prev_event.idx.unwrap_or(0) + prev_event.repeat.unwrap_or(0)
                    == event.idx.unwrap_or(0)
            {
                let mut updated = event;
                updated.idx = None;
                new_events.push(updated);
            } else {
                new_events.push(event);
            }
        }
    }
    new_events
}

/// `remove_redundant_repeat`.
fn remove_redundant_repeat(events: Vec<ParamEvent>) -> Vec<ParamEvent> {
    events
        .into_iter()
        .map(|event| {
            if event.repeat.unwrap_or(0) > 1 {
                event
            } else {
                ParamEvent {
                    repeat: None,
                    ..event
                }
            }
        })
        .collect()
}
