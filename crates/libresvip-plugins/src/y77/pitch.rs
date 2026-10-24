//! Port of the parts of `libresvip.model.pitch_simulator` used by the Yuan77
//! generator (`PortamentoPitch.no_portamento()` only).
//!
//! This is a self-contained copy of the no-portamento simulator; plugins must
//! not import each other's modules, so the shared core logic is duplicated.

use libresvip_core::core::tick_counter::find_bar_index;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{Note, ParamCurve, TimeSignature};
use libresvip_core::Result;

struct KeyInterval {
    start: f64,
    end: f64,
    key: i64,
}

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
    pub fn new(
        synchronizer: TimeSynchronizer,
        note_list: &[Note],
        time_signature_list: &[TimeSignature],
    ) -> Result<Self> {
        let mut intervals: Vec<KeyInterval> = Vec::new();
        if !note_list.is_empty() {
            let mut previous = &note_list[0];
            let mut previous_head = synchronizer.get_actual_secs_from_ticks(previous.start_pos);
            let mut previous_dur =
                synchronizer.get_duration_secs_from_ticks(previous.start_pos, previous.end_pos());
            let mut previous_portamento = 0.0f64;
            let max_portamento_time = 0.0f64;
            intervals.push(KeyInterval {
                start: 0.0,
                end: previous_head,
                key: previous.key_number,
            });
            let mut prev_portamento_end = previous_head;

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

    pub fn pitch_at_ticks(&self, ticks: i64) -> Option<f64> {
        self.pitch_at_secs(self.synchronizer.get_actual_secs_from_ticks(ticks))
    }

    pub fn pitch_at_secs(&self, secs: f64) -> Option<f64> {
        if let Some(value) = self.pitch_override_at_secs(secs) {
            return Some(value);
        }
        // The intervals are appended in ascending order. The generator samples
        // every note 500 times, so a linear scan here is quadratic in the song
        // length; look the candidate up instead.
        let index = self
            .intervals
            .partition_point(|interval| interval.start <= secs);
        let interval = self.intervals.get(index.checked_sub(1)?)?;
        if secs < interval.end {
            // Fixed deviation from LibreSVIP 2.9.2: `if interval.key` treated
            // key 0 (C-1) as "no pitch"; it is a real note.
            return Some(interval.key as f64 * 100.0);
        }
        None
    }

    fn pitch_override_at_secs(&self, secs: f64) -> Option<f64> {
        // Segments are appended in ascending order by `merge_pitch_curve`.
        let index = self
            .segments
            .partition_point(|segment| segment.start <= secs);
        let segment = self.segments.get(index.checked_sub(1)?)?;
        if secs < segment.end {
            return Some(linear_interpolation(
                secs,
                (segment.start, segment.start_value),
                (segment.end, segment.end_value),
            ));
        }
        None
    }
}

fn split_at_interrupt(
    points: &[libresvip_core::model::point::Point],
) -> Vec<&[libresvip_core::model::point::Point]> {
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

fn linear_interpolation(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let r = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * r
}
