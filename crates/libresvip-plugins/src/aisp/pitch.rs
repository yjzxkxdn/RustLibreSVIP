//! The subset of `libresvip.model.pitch_simulator` needed by the AISingers
//! generator (`PortamentoPitch.no_portamento()`).
//!
//! With no portamento the pitch simulator degenerates to a piecewise constant
//! "nearest note" function plus the merged edited pitch curve, but the interval
//! boundaries still matter: they sit exactly halfway between adjacent notes.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::Note;
use libresvip_core::model::point::Point;

#[derive(Clone, Debug, Default)]
struct ConstantIntervals {
    entries: Vec<(f64, f64, f64)>,
}

impl ConstantIntervals {
    /// `PiecewiseIntervalDict.__setitem__` for a closed-open interval: empty
    /// intervals are ignored and a new value replaces every existing slice it
    /// overlaps.
    fn insert(&mut self, lower: f64, upper: f64, value: f64) {
        if !(lower < upper) {
            return;
        }
        let mut next = Vec::with_capacity(self.entries.len() + 2);
        for (start, end, previous) in self.entries.drain(..) {
            if end <= lower || start >= upper {
                next.push((start, end, previous));
                continue;
            }
            if start < lower {
                next.push((start, lower, previous));
            }
            if upper < end {
                next.push((upper, end, previous));
            }
        }
        next.push((lower, upper, value));
        next.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.entries = next;
    }

    fn get(&self, position: f64) -> Option<f64> {
        let index = self
            .entries
            .partition_point(|(start, _, _)| *start <= position);
        let (start, end, value) = *self.entries.get(index.checked_sub(1)?)?;
        if position >= start && position < end {
            Some(value)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Default)]
struct LinearIntervals {
    entries: Vec<(f64, f64, f64, f64)>,
}

impl LinearIntervals {
    fn insert(&mut self, lower: f64, upper: f64, start_value: f64, end_value: f64) {
        if !(lower < upper) {
            return;
        }
        let mut next = Vec::with_capacity(self.entries.len() + 2);
        for (start, end, first, last) in self.entries.drain(..) {
            if end <= lower || start >= upper {
                next.push((start, end, first, last));
                continue;
            }
            if start < lower {
                next.push((start, lower, first, last));
            }
            if upper < end {
                next.push((upper, end, first, last));
            }
        }
        next.push((lower, upper, start_value, end_value));
        next.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.entries = next;
    }

    fn get(&self, position: f64) -> Option<f64> {
        let index = self
            .entries
            .partition_point(|(start, _, _, _)| *start <= position);
        let (start, end, first, last) = *self.entries.get(index.checked_sub(1)?)?;
        if position >= start && position < end {
            Some(linear_interpolation(position, start, first, end, last))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PitchSegment {
    start: f64,
    end: f64,
    start_value: f64,
    end_value: f64,
}

#[derive(Clone, Debug, Default)]
pub struct PitchSimulator {
    note_intervals: ConstantIntervals,
    pitch_intervals: LinearIntervals,
    segments: Vec<PitchSegment>,
    segment_starts: Vec<f64>,
    segment_ends: Vec<f64>,
}

fn linear_interpolation(position: f64, start: f64, start_value: f64, end: f64, end_value: f64) -> f64 {
    let ratio = (position - start) / (end - start);
    start_value + (end_value - start_value) * ratio
}

impl PitchSimulator {
    /// `PitchSimulator(synchronizer, PortamentoPitch.no_portamento(), note_list, time_signatures)`.
    pub fn no_portamento(note_list: &[Note], synchronizer: &TimeSynchronizer) -> Result<Self> {
        let mut simulator = Self::default();
        let Some(first_note) = note_list.first() else {
            return Ok(simulator);
        };
        let mut current_note = first_note;
        let mut current_head = synchronizer.get_actual_secs_from_ticks(current_note.start_pos);
        let mut current_duration = synchronizer.get_duration_secs_from_ticks(
            current_note.start_pos,
            current_note.end_pos(),
        );
        simulator
            .note_intervals
            .insert(0.0, current_head, current_note.key_number as f64);
        let mut previous_end = current_head;
        for next_note in &note_list[1..] {
            if current_note.end_pos() > next_note.start_pos {
                return Err(LibreSvipError::NotesOverlapped);
            }
            let next_head = synchronizer.get_actual_secs_from_ticks(next_note.start_pos);
            let next_duration = synchronizer
                .get_duration_secs_from_ticks(next_note.start_pos, next_note.end_pos());
            // `no_portamento` has max_inter_time_in_secs = 0 and
            // max_inter_time_percent = 0, so both branches of the Python code
            // collapse to `middle_time`.
            let middle_time = (next_head + current_head + current_duration) / 2.0;
            simulator.note_intervals.insert(
                previous_end,
                middle_time,
                current_note.key_number as f64,
            );
            current_note = next_note;
            current_head = next_head;
            current_duration = next_duration;
            previous_end = middle_time;
        }
        simulator.note_intervals.insert(
            previous_end,
            f64::INFINITY,
            current_note.key_number as f64,
        );
        Ok(simulator)
    }

    /// `PitchSimulator.merge_pitch_curve`.
    pub fn merge_pitch_curve(
        &mut self,
        points: &[Point],
        first_bar_length: i64,
        synchronizer: &TimeSynchronizer,
    ) {
        self.pitch_intervals = LinearIntervals::default();
        self.segments.clear();
        for part in points.split(|point| point.y == -100) {
            if part.is_empty() {
                continue;
            }
            for pair in part.windows(2) {
                let previous = pair[0];
                let point = pair[1];
                let start_time = synchronizer
                    .get_actual_secs_from_ticks(previous.x - first_bar_length);
                let end_time =
                    synchronizer.get_actual_secs_from_ticks(point.x - first_bar_length);
                if start_time >= end_time {
                    continue;
                }
                self.pitch_intervals.insert(
                    start_time,
                    end_time,
                    previous.y as f64,
                    point.y as f64,
                );
                self.segments.push(PitchSegment {
                    start: start_time,
                    end: end_time,
                    start_value: previous.y as f64,
                    end_value: point.y as f64,
                });
            }
        }
        self.segment_starts = self.segments.iter().map(|segment| segment.start).collect();
        self.segment_ends = self.segments.iter().map(|segment| segment.end).collect();
    }

    pub fn pitch_at_ticks(&self, ticks: i64, synchronizer: &TimeSynchronizer) -> Option<f64> {
        self.pitch_at_secs(synchronizer.get_actual_secs_from_ticks(ticks))
    }

    fn pitch_at_secs(&self, secs: f64) -> Option<f64> {
        if let Some(value) = self.pitch_override_at_secs(secs) {
            return Some(value);
        }
        if let Some(value) = self.note_intervals.get(secs) {
            if value != 0.0 {
                return Some(value * 100.0);
            }
        }
        None
    }

    fn pitch_override_at_secs(&self, secs: f64) -> Option<f64> {
        if !self.segments.is_empty() {
            if let Some(index) = self.find_pitch_interval_index(secs) {
                let segment = self.segments[index];
                return Some(linear_interpolation(
                    secs,
                    segment.start,
                    segment.start_value,
                    segment.end,
                    segment.end_value,
                ));
            }
        }
        if let Some(value) = self.pitch_intervals.get(secs) {
            if value != 0.0 {
                return Some(value);
            }
        }
        None
    }

    fn find_pitch_interval_index(&self, secs: f64) -> Option<usize> {
        if self.segments.is_empty() {
            return None;
        }
        let mut left = 0usize;
        let mut right = self.segments.len() - 1;
        while left <= right {
            let middle = (left + right) / 2;
            if secs < self.segment_starts[middle] {
                if middle == 0 {
                    break;
                }
                right = middle - 1;
            } else if secs >= self.segment_ends[middle] {
                left = middle + 1;
            } else {
                return Some(middle);
            }
        }
        None
    }
}
