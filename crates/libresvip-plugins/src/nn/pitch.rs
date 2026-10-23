//! Port of the parts of `libresvip.model.pitch_simulator` used by the
//! NIAONiao generator (`PortamentoPitch.no_portamento()` only).
//!
//! The Python class supports a Vocaloid-style portamento mode too, but that mode
//! is never selected by this plugin, so only the no-portamento branch is
//! reproduced (including its exact interval boundaries).

use libresvip_core::core::tick_counter::find_bar_index;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{Note, ParamCurve, TimeSignature};
use libresvip_core::Result;

/// A closed-open key interval (`key_number` applies for `start <= x < end`).
struct KeyInterval {
    start: f64,
    end: f64,
    key: i64,
}

/// A closed-open linear pitch segment from the merged pitch curve.
struct PitchSegment {
    start: f64,
    end: f64,
    start_value: f64,
    end_value: f64,
}

pub struct PitchSimulator {
    synchronizer: TimeSynchronizer,
    intervals: Vec<KeyInterval>,
    segments: Vec<PitchSegment>,
}

impl PitchSimulator {
    /// `PitchSimulator(synchronizer, portamento=no_portamento(), note_list,
    /// time_signature_list)` followed by the `__post_init__` interval build.
    pub fn new(
        synchronizer: TimeSynchronizer,
        note_list: &[Note],
        time_signature_list: &[TimeSignature],
    ) -> Result<Self> {
        let mut intervals: Vec<KeyInterval> = Vec::new();
        if !note_list.is_empty() {
            let current_note = &note_list[0];
            let current_head = synchronizer.get_actual_secs_from_ticks(current_note.start_pos);
            let current_dur = synchronizer
                .get_duration_secs_from_ticks(current_note.start_pos, current_note.end_pos());
            // No portamento: both the percent and the maximum time are zero.
            let current_portamento = 0.0f64;
            let max_portamento_time = 0.0f64;

            intervals.push(KeyInterval {
                start: 0.0,
                end: current_head,
                key: current_note.key_number,
            });
            let mut prev_portamento_end = current_head;
            let mut previous = current_note;
            let mut previous_head = current_head;
            let mut previous_dur = current_dur;
            let mut previous_portamento = current_portamento;

            for next_note in &note_list[1..] {
                if previous.end_pos() > next_note.start_pos {
                    let bar = find_bar_index(time_signature_list, next_note.start_pos)?;
                    return Err(libresvip_core::LibreSvipError::msg(format!(
                        "Notes overlapped near bar {bar}"
                    )));
                }
                let next_head = synchronizer.get_actual_secs_from_ticks(next_note.start_pos);
                let next_dur = synchronizer
                    .get_duration_secs_from_ticks(next_note.start_pos, next_note.end_pos());
                let next_portamento = 0.0f64;
                let interval = (next_head - previous_head - previous_dur) / 2.0;
                let middle_time = (next_head + previous_head + previous_dur) / 2.0;
                let (portamento_start, portamento_end) = if interval <= max_portamento_time {
                    (
                        middle_time - previous_portamento,
                        middle_time + next_portamento,
                    )
                } else {
                    (
                        middle_time - max_portamento_time,
                        middle_time + max_portamento_time,
                    )
                };
                intervals.push(KeyInterval {
                    start: prev_portamento_end,
                    end: portamento_start,
                    key: previous.key_number,
                });
                if previous.key_number == next_note.key_number {
                    intervals.push(KeyInterval {
                        start: portamento_start,
                        end: portamento_end,
                        key: previous.key_number,
                    });
                }
                // `previous.key_number != next.key_number` with an empty
                // portamento range produces no interval at all.

                previous = next_note;
                previous_head = next_head;
                previous_dur = next_dur;
                previous_portamento = next_portamento;
                prev_portamento_end = portamento_end;
            }
            intervals.push(KeyInterval {
                start: prev_portamento_end,
                end: f64::INFINITY,
                key: previous.key_number,
            });
        }
        Ok(Self {
            synchronizer,
            intervals,
            segments: Vec::new(),
        })
    }

    /// `merge_pitch_curve(pitch_curve, first_bar_length)`.
    pub fn merge_pitch_curve(&mut self, pitch_curve: &ParamCurve, first_bar_length: i64) {
        self.segments.clear();
        for part in split_at_interrupt(&pitch_curve.points) {
            for pair in part.windows(2) {
                let previous = pair[0];
                let point = pair[1];
                let start_time = self
                    .synchronizer
                    .get_actual_secs_from_ticks(previous.x - first_bar_length);
                let end_time = self
                    .synchronizer
                    .get_actual_secs_from_ticks(point.x - first_bar_length);
                if start_time >= end_time {
                    continue;
                }
                self.segments.push(PitchSegment {
                    start: start_time,
                    end: end_time,
                    start_value: previous.y as f64,
                    end_value: point.y as f64,
                });
            }
        }
    }

    /// `pitch_at_ticks`.
    pub fn pitch_at_ticks(&self, ticks: i64) -> Option<f64> {
        self.pitch_at_secs(self.synchronizer.get_actual_secs_from_ticks(ticks))
    }

    /// `pitch_at_secs`.
    pub fn pitch_at_secs(&self, secs: f64) -> Option<f64> {
        if let Some(value) = self.pitch_override_at_secs(secs) {
            return Some(value);
        }
        for interval in self.intervals.iter().rev() {
            if interval.start <= secs && secs < interval.end {
                // Python: `if value := dict.get(secs)` — a zero key is falsy.
                if interval.key == 0 {
                    return None;
                }
                return Some(interval.key as f64 * 100.0);
            }
        }
        None
    }

    fn pitch_override_at_secs(&self, secs: f64) -> Option<f64> {
        for segment in &self.segments {
            if segment.start <= secs && secs < segment.end {
                return Some(linear_interpolation(
                    secs,
                    (segment.start, segment.start_value),
                    (segment.end, segment.end_value),
                ));
            }
        }
        None
    }
}

/// `more_itertools.split_at(points, lambda point: point.y == -100)`.
fn split_at_interrupt(points: &[libresvip_core::model::point::Point]) -> Vec<&[libresvip_core::model::point::Point]> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    for (index, point) in points.iter().enumerate() {
        if point.y == -100 {
            if start < index {
                parts.push(&points[start..index]);
            }
            start = index + 1;
        }
    }
    if start < points.len() {
        parts.push(&points[start..]);
    }
    parts
}

/// `linear_interpolation(x, start, end)`.
fn linear_interpolation(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let r = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * r
}
