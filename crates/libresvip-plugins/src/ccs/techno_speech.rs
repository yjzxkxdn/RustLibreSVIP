//! Port of `libresvip.model.techno_speech_pitch` as used by CeVIO.

use libresvip_core::core::tick_counter::shift_tempo_list;
use libresvip_core::model::base::{ParamCurve, SongTempo};
use libresvip_core::model::point::Point;

use super::interval::{
    py_round, ConstantIntervals, Sync, WaveIntervals, MIN_DATA_LENGTH, TEMP_VALUE_AS_NULL,
    TIME_UNIT_AS_TICKS_PER_BPM,
};

/// A `Data` element of a CeVIO parameter.
#[derive(Clone, Copy, Debug)]
pub struct ParamEvent {
    pub idx: Option<i64>,
    pub repeat: Option<i64>,
    pub value: f64,
}

#[derive(Clone, Copy, Debug)]
struct EventFloat {
    idx: Option<f64>,
    repeat: Option<f64>,
    value: Option<f64>,
}

impl EventFloat {
    fn from_event(event: &ParamEvent) -> Self {
        Self {
            idx: event.idx.map(|idx| idx as f64),
            repeat: event.repeat.map(|repeat| repeat as f64),
            value: Some(event.value),
        }
    }
}

fn append_ending_points(events: &[ParamEvent]) -> Vec<ParamEvent> {
    let mut result = Vec::new();
    let mut next_pos: Option<i64> = None;
    for event in events {
        let Some(pos) = event.idx.or(next_pos) else {
            continue;
        };
        let length = event.repeat.unwrap_or(1);
        if let Some(previous) = next_pos {
            if previous < pos {
                result.push(ParamEvent {
                    idx: Some(previous),
                    repeat: None,
                    value: TEMP_VALUE_AS_NULL,
                });
            }
        }
        result.push(ParamEvent {
            idx: Some(pos),
            repeat: Some(length),
            value: event.value,
        });
        next_pos = Some(pos + length);
    }
    if let Some(previous) = next_pos {
        result.push(ParamEvent {
            idx: Some(previous),
            repeat: None,
            value: TEMP_VALUE_AS_NULL,
        });
    }
    result
}

/// `expand(tempos, tick_prefix)`.
pub fn expand(tempos: &[SongTempo], tick_prefix: i64) -> Vec<(i64, i64, f64)> {
    let mut result: Vec<(i64, i64, f64)> = Vec::new();
    for (index, tempo) in tempos.iter().enumerate() {
        if index == 0 {
            result.push((0, tick_prefix, tempo.bpm));
        } else {
            let (last_pos, last_tick_pos, last_bpm) = *result.last().unwrap_or(&(0, 0, tempo.bpm));
            let ticks_in_time_unit = TIME_UNIT_AS_TICKS_PER_BPM * last_bpm;
            let new_pos =
                last_pos as f64 + (tempo.position - last_tick_pos) as f64 / ticks_in_time_unit;
            result.push((new_pos as i64, tempo.position, tempo.bpm));
        }
    }
    result
}

fn normalize_to_tick(
    events: &[ParamEvent],
    tempo_list: &[SongTempo],
    tick_prefix: i64,
    expanded_tempos: Option<&[(i64, i64, f64)]>,
) -> Vec<EventFloat> {
    let owned;
    let tempos = match expanded_tempos {
        Some(tempos) => tempos,
        None => {
            owned = expand(tempo_list, tick_prefix);
            &owned
        }
    };
    let mut result = Vec::new();
    let mut current_tempo_index = 0usize;
    let mut next_pos = 0.0f64;
    let mut next_tick_pos = 0.0f64;
    for event in events {
        let event = EventFloat::from_event(event);
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
                - (tempos[current_tempo_index].1 as f64).max(tick_pos);
            remaining_repeat -= tempos[current_tempo_index + 1].0 as f64
                - (tempos[current_tempo_index].0 as f64).max(pos);
            current_tempo_index += 1;
        }
        repeat_in_ticks += remaining_repeat * TIME_UNIT_AS_TICKS_PER_BPM
            * tempos[current_tempo_index].2;
        next_pos = pos + repeat;
        next_tick_pos = tick_pos + repeat_in_ticks;
        result.push(EventFloat {
            idx: Some(tick_pos),
            repeat: Some(repeat_in_ticks),
            value: event.value,
        });
    }
    result
        .into_iter()
        .map(|tick| EventFloat {
            idx: tick.idx.map(|idx| idx + tick_prefix as f64),
            repeat: tick.repeat,
            value: match tick.value {
                Some(value) if value == TEMP_VALUE_AS_NULL => None,
                other => other,
            },
        })
        .collect()
}

fn shape_events(events: &[EventFloat]) -> Vec<EventFloat> {
    let mut result: Vec<EventFloat> = Vec::new();
    for event in events {
        if let Some(repeat) = event.repeat {
            if repeat > 0.0 {
                if let Some(last) = result.last_mut() {
                    if last.idx == event.idx {
                        *last = *event;
                    } else {
                        result.push(*event);
                    }
                } else {
                    result.push(*event);
                }
            }
        }
    }
    result
}

fn continuous_parts(
    events: &[ParamEvent],
    synchronizer: &Sync,
    tick_prefix: i64,
    expanded_tempos: Option<&[(i64, i64, f64)]>,
) -> Vec<Vec<EventFloat>> {
    let mut parts: Vec<Vec<ParamEvent>> = Vec::new();
    for event in events {
        if event.idx.is_some() || parts.is_empty() {
            parts.push(Vec::new());
        }
        parts.last_mut().unwrap().push(*event);
    }
    parts
        .iter()
        .map(|part| {
            normalize_to_tick(
                &append_ending_points(part),
                &synchronizer.tempo_list,
                tick_prefix,
                expanded_tempos,
            )
        })
        .collect()
}

fn event_repeat_or_one(event: &EventFloat) -> f64 {
    event.repeat.unwrap_or(1.0)
}

fn build_param_interval_dict(
    events: &[ParamEvent],
    synchronizer: &Sync,
    tick_prefix: i64,
    expanded_tempos: Option<&[(i64, i64, f64)]>,
) -> ConstantIntervals {
    let mut dict = ConstantIntervals::default();
    for part in continuous_parts(events, synchronizer, tick_prefix, expanded_tempos) {
        let mut previous: Option<EventFloat> = None;
        for next in &part {
            let Some(next_idx) = next.idx else {
                continue;
            };
            let next_start = synchronizer.secs_at(next_idx - tick_prefix as f64);
            if let Some(previous) = previous {
                if let Some(previous_idx) = previous.idx {
                    let previous_end = synchronizer.secs_at(
                        previous_idx + event_repeat_or_one(&previous) - tick_prefix as f64,
                    );
                    if previous_end != 0.0 && previous_end < next_start {
                        dict.insert(previous_end, next_start, next.value);
                    }
                }
            }
            let end = synchronizer
                .secs_at(next_idx + event_repeat_or_one(next) - tick_prefix as f64);
            dict.insert(next_start, end, next.value);
            previous = Some(*next);
        }
    }
    dict
}

fn build_wave_interval_dict(
    events: &[ParamEvent],
    synchronizer: &Sync,
    tick_prefix: i64,
    expanded_tempos: Option<&[(i64, i64, f64)]>,
) -> WaveIntervals {
    let mut dict = WaveIntervals::default();
    let mut omega = std::f64::consts::TAU * 6.0;
    for part in continuous_parts(events, synchronizer, tick_prefix, expanded_tempos) {
        let mut phase = 0.0f64;
        let mut previous: Option<EventFloat> = None;
        for next in &part {
            let Some(next_idx) = next.idx else {
                continue;
            };
            let next_start = synchronizer.secs_at(next_idx - tick_prefix as f64);
            let next_end =
                synchronizer.secs_at(next_idx + event_repeat_or_one(next) - tick_prefix as f64);
            if let Some(previous) = previous {
                if let Some(previous_idx) = previous.idx {
                    let previous_end = synchronizer.secs_at(
                        previous_idx + event_repeat_or_one(&previous) - tick_prefix as f64,
                    );
                    if previous_end != 0.0 && previous_end < next_start {
                        let previous_start =
                            synchronizer.secs_at(previous_idx - tick_prefix as f64);
                        let value = (omega * (previous_end - previous_start) + phase).sin();
                        dict.insert_constant(previous_end, next_start, value);
                    }
                }
            }
            omega = std::f64::consts::TAU
                * match next.value {
                    Some(value) if value != 0.0 => value,
                    _ => 6.0,
                };
            dict.insert_wave(next_start, next_end, next_start, omega, phase);
            phase += (next_end - next_start) * omega;
            previous = Some(*next);
        }
    }
    dict
}

fn hz2midi(hz: f64) -> f64 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// `pitch_from_techno_speech_track`.
pub fn pitch_from_techno_speech_track(
    events: &[ParamEvent],
    tempos: &[SongTempo],
    tick_prefix: i64,
    vibrato_amplitude_events: &[ParamEvent],
    vibrato_frequency_events: &[ParamEvent],
) -> Option<ParamCurve> {
    let synchronizer = Sync::new(tempos);
    let expanded_tempos = expand(tempos, tick_prefix);
    let amplitude_dict = build_param_interval_dict(
        vibrato_amplitude_events,
        &synchronizer,
        tick_prefix,
        Some(&expanded_tempos),
    );
    let wave_dict = build_wave_interval_dict(
        vibrato_frequency_events,
        &synchronizer,
        tick_prefix,
        Some(&expanded_tempos),
    );
    let normalized = shape_events(&normalize_to_tick(
        &append_ending_points(events),
        tempos,
        tick_prefix,
        Some(&expanded_tempos),
    ));
    let mut converted_points: Vec<Point> = vec![Point::start_point()];
    let mut current_value = -100i64;
    let mut next_pos: Option<f64> = None;
    for event in &normalized {
        let Some(idx) = event.idx else {
            continue;
        };
        let pos = idx as i64 - tick_prefix;
        let mut secs = synchronizer.secs_at(pos as f64);
        let length = event.repeat.unwrap_or(0.0);
        let value = match event.value {
            None => Some(-100i64),
            Some(value) => {
                if value > 709.782712893384 {
                    None
                } else {
                    let hz = value.exp();
                    let midi = hz2midi(hz);
                    if midi.is_finite() {
                        Some(py_round(midi * 100.0))
                    } else {
                        None
                    }
                }
            }
        };
        if let Some(value) = value {
            if value != current_value || next_pos != Some(pos as f64) {
                converted_points.push(Point::new(py_round(pos as f64), value));
                if value == -100 {
                    converted_points.push(Point::new(py_round(pos as f64), value));
                }
                current_value = value;
            }
            let secs_step = synchronizer.duration(pos as f64, pos as f64 + 5.0);
            let end_x = (pos as f64 + length) as i64;
            let mut pos_x = pos;
            while pos_x < end_x {
                let Some(diff) = wave_dict.get(secs) else {
                    break;
                };
                if diff == 0.0 {
                    break;
                }
                let amplitude = amplitude_dict.get_with_default(secs, 1.0);
                converted_points.push(Point::new(
                    pos_x,
                    py_round(value as f64 + diff * amplitude),
                ));
                secs += secs_step;
                pos_x += 5;
            }
        }
        next_pos = Some(pos as f64 + length);
    }
    converted_points.push(Point::end_point());
    if converted_points.len() > 2 {
        Some(ParamCurve::new(converted_points))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Generation (`generate_for_techno_speech`)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct GeneratedEvent {
    idx: Option<i64>,
    repeat: Option<i64>,
    value: f64,
}

pub struct TrackPitchData {
    pub events: Vec<ParamEvent>,
    pub length: i64,
}

impl TrackPitchData {
    fn length(&self) -> i64 {
        if self.events.is_empty() {
            return MIN_DATA_LENGTH;
        }
        let last_has_index = self
            .events
            .iter()
            .rposition(|event| event.idx.is_some());
        let Some(last) = last_has_index else {
            return MIN_DATA_LENGTH;
        };
        let last_idx = self.events[last].idx.unwrap_or(0);
        let repeat_sum: i64 = self.events[last..]
            .iter()
            .map(|event| event.repeat.unwrap_or(1))
            .sum();
        last_idx + repeat_sum + MIN_DATA_LENGTH
    }
}

fn denormalize_from_tick(
    events_with_full_params: &[EventFloat],
    tempos_in_ticks: &[SongTempo],
    tick_prefix: i64,
) -> Vec<GeneratedEvent> {
    let shifted = shift_tempo_list(tempos_in_ticks, tick_prefix);
    let tempos = expand(&shifted, tick_prefix);
    let shifted_events: Vec<EventFloat> = events_with_full_params
        .iter()
        .map(|event| EventFloat {
            idx: event.idx.map(|idx| idx + tick_prefix as f64),
            repeat: event.repeat,
            value: event.value,
        })
        .collect();
    let mut events = Vec::new();
    let mut current_tempo_index = 0usize;
    for event in &shifted_events {
        let Some(idx) = event.idx else {
            continue;
        };
        while current_tempo_index + 1 < tempos.len()
            && (tempos[current_tempo_index + 1].1 as f64) < idx
        {
            current_tempo_index += 1;
        }
        let ticks_per_time_unit =
            tempos[current_tempo_index].2 * TIME_UNIT_AS_TICKS_PER_BPM;
        let pos = tempos[current_tempo_index].0 as f64
            + (idx - tempos[current_tempo_index].1 as f64) / ticks_per_time_unit;
        let mut repeat_in_ticks = event.repeat.unwrap_or(0.0);
        let mut repeat = 0.0f64;
        while current_tempo_index + 1 < tempos.len()
            && (tempos[current_tempo_index + 1].1 as f64) < idx + repeat_in_ticks
        {
            repeat += tempos[current_tempo_index + 1].0 as f64
                - (tempos[current_tempo_index].0 as f64).max(pos);
            repeat_in_ticks -= tempos[current_tempo_index + 1].1 as f64
                - (tempos[current_tempo_index].1 as f64).max(idx);
            current_tempo_index += 1;
        }
        repeat += repeat_in_ticks / (TIME_UNIT_AS_TICKS_PER_BPM * tempos[current_tempo_index].2);
        events.push(GeneratedEvent {
            idx: Some(py_round(pos)),
            repeat: Some(py_round(repeat.max(1.0))),
            value: event.value.unwrap_or(0.0),
        });
    }
    events
}

fn restore_connection(
    events: &[GeneratedEvent],
    are_events_connected_to_next: &[bool],
) -> Vec<GeneratedEvent> {
    let mut new_events = Vec::new();
    for (index, event) in events.iter().enumerate() {
        let next = events.get(index + 1);
        let connected = are_events_connected_to_next
            .get(index)
            .copied()
            .unwrap_or(false);
        if next.is_none() || !connected {
            new_events.push(*event);
        } else {
            let mut replaced = *event;
            if let (Some(next_idx), Some(current_idx)) = (next.unwrap().idx, event.idx) {
                replaced.repeat = Some(next_idx - current_idx);
            }
            new_events.push(replaced);
        }
    }
    new_events
}

fn merge_events_if_possible(events: &[GeneratedEvent]) -> Vec<GeneratedEvent> {
    let mut new_events: Vec<GeneratedEvent> = Vec::new();
    for event in events {
        let mut event = *event;
        if new_events.is_empty() {
            new_events.push(event);
            continue;
        }
        let last_event = *new_events.last().unwrap();
        let overlapped_len =
            last_event.idx.unwrap_or(0) + last_event.repeat.unwrap_or(0) - event.idx.unwrap_or(0);
        let mut last = last_event;
        if overlapped_len > 0 {
            new_events.last_mut().unwrap().repeat =
                Some(event.idx.unwrap_or(0) - last_event.idx.unwrap_or(0));
            event = GeneratedEvent {
                idx: Some(overlapped_len + event.idx.unwrap_or(0)),
                repeat: Some(event.repeat.unwrap_or(0) - overlapped_len),
                value: event.value,
            };
            last = GeneratedEvent {
                idx: event.idx,
                repeat: Some(overlapped_len),
                value: event.value + last_event.value,
            };
            new_events.push(last);
        }
        if last.value == event.value
            && last.idx.unwrap_or(0) + last.repeat.unwrap_or(0) == event.idx.unwrap_or(0)
        {
            let combined = last.repeat.unwrap_or(0) + event.repeat.unwrap_or(0);
            new_events.last_mut().unwrap().repeat = Some(combined);
        } else {
            new_events.push(event);
        }
    }
    new_events
}

fn remove_redundant_index(events: &[GeneratedEvent]) -> Vec<GeneratedEvent> {
    let mut new_events: Vec<GeneratedEvent> = Vec::new();
    for event in events {
        if let Some(previous) = new_events.last() {
            if let (Some(idx), Some(repeat)) = (previous.idx, previous.repeat) {
                if idx + repeat == event.idx.unwrap_or(i64::MIN) {
                    let mut replaced = *event;
                    replaced.idx = None;
                    new_events.push(replaced);
                    continue;
                }
            }
        }
        new_events.push(*event);
    }
    new_events
}

fn remove_redundant_repeat(events: &[GeneratedEvent]) -> Vec<GeneratedEvent> {
    events
        .iter()
        .map(|event| {
            if event.repeat.unwrap_or(1) > 1 {
                *event
            } else {
                GeneratedEvent {
                    repeat: None,
                    ..*event
                }
            }
        })
        .collect()
}

fn midi2hz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}

pub fn generate_for_techno_speech(
    pitch: &ParamCurve,
    tempos: &[SongTempo],
    tick_prefix: i64,
) -> Option<TrackPitchData> {
    let points = &pitch.points;
    let mut events_with_full_params: Vec<EventFloat> = Vec::new();
    for (index, this_point) in points.iter().enumerate() {
        let next_point = points.get(index + 1);
        let end_tick = next_point.map(|point| point.x - tick_prefix);
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
        if value.is_some() && (next_point.is_none() || next_point.unwrap().y != -100) {
            events_with_full_params.push(EventFloat {
                idx: Some(point_index as f64),
                repeat: Some(repeat),
                value,
            });
        }
    }
    let are_events_connected_to_next: Vec<bool> = (0..events_with_full_params.len())
        .map(|index| {
            let this_event = events_with_full_params[index];
            match events_with_full_params.get(index + 1) {
                Some(next_event) => {
                    this_event.idx.unwrap_or(0.0) + this_event.repeat.unwrap_or(0.0)
                        >= next_event.idx.unwrap_or(0.0)
                }
                None => false,
            }
        })
        .collect();
    let events = denormalize_from_tick(&events_with_full_params, tempos, tick_prefix);
    let events = restore_connection(&events, &are_events_connected_to_next);
    let events = merge_events_if_possible(&events);
    let events = remove_redundant_index(&events);
    let events = remove_redundant_repeat(&events);
    if events.is_empty() {
        return None;
    }
    let data = TrackPitchData {
        events: events
            .iter()
            .map(|event| ParamEvent {
                idx: event.idx,
                repeat: event.repeat,
                value: event.value,
            })
            .collect(),
        length: 0,
    };
    let length = data.length();
    Some(TrackPitchData {
        events: data.events,
        length,
    })
}
