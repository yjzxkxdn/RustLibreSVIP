//! Port of `libresvip.plugins.ppsf.ppsf_interval_dict`.
//!
//! Python builds a `portion.IntervalDict` mapping tick ranges to either a
//! constant note number or a cosine-eased interpolation between two note
//! numbers. `IntervalDict.__setitem__` overwrites the parts of earlier ranges
//! that it covers, so for point lookups the *last* inserted range containing a
//! tick wins; this implementation reproduces that with an insertion-ordered list.

use super::model::{PpsfDvlTrackEvent, PpsfNote};

/// Value stored for a tick range.
#[derive(Clone, Copy, Debug)]
pub enum IntervalValue {
    Constant(f64),
    /// `cosine_easing_in_out_interpolation` between two points.
    Interpolation {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
}

impl IntervalValue {
    pub fn evaluate(self, x: f64) -> f64 {
        match self {
            IntervalValue::Constant(value) => value,
            IntervalValue::Interpolation { x0, y0, x1, y1 } => {
                if x1 == x0 {
                    return y0;
                }
                let r = (x - x0) / (x1 - x0);
                let eased = (1.0 - (r * std::f64::consts::PI).cos()) / 2.0;
                y0 + (y1 - y0) * eased
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Interval {
    start: f64,
    end: f64,
    value: IntervalValue,
}

/// `PiecewiseIntervalDict` specialised to numeric keys.
#[derive(Clone, Debug, Default)]
pub struct KeyIntervalDict {
    intervals: Vec<Interval>,
}

impl KeyIntervalDict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, start: f64, end: f64, value: IntervalValue) {
        self.intervals.push(Interval { start, end, value });
    }

    /// `interval_dict.get(pos)`: `None` when no interval covers the tick.
    pub fn get(&self, pos: i64) -> Option<f64> {
        let key = pos as f64;
        self.intervals
            .iter()
            .rev()
            .find(|interval| interval.start <= key && key < interval.end)
            .map(|interval| interval.value.evaluate(key))
    }
}

/// `ppsf_key_interval_dict(event_list, note_list)`.
///
/// Note the (confusing but faithful) naming in the Python source: the
/// `PpsfDvlTrackEvent` supplies positions and note numbers, while the
/// `PpsfNote` supplies the portamento settings.
pub fn ppsf_key_interval_dict(
    event_list: &[PpsfDvlTrackEvent],
    note_list: &[PpsfNote],
) -> KeyIntervalDict {
    let mut interval_dict = KeyIntervalDict::new();
    if event_list.len() == 1 {
        interval_dict.insert(0.0, f64::INFINITY, IntervalValue::Constant(
            event_list[0].note_number as f64,
        ));
        return interval_dict;
    }
    let count = event_list.len().min(note_list.len());
    if count < 2 {
        return interval_dict;
    }
    for index in 0..count - 1 {
        let prev_event = &event_list[index];
        let prev_note = &note_list[index];
        let next_event = &event_list[index + 1];
        let next_note = &note_list[index + 1];
        let prev_portamento_end = prev_event.pos
            + prev_note.portamento_offset.unwrap_or(-120)
            + prev_note.portamento_length.unwrap_or(180);
        if index == 0 {
            interval_dict.insert(
                0.0,
                prev_portamento_end as f64,
                IntervalValue::Constant(prev_event.note_number as f64),
            );
        }
        if next_note.portamento_length.unwrap_or(180) != 0 {
            let next_portamento_start =
                next_event.pos + next_note.portamento_offset.unwrap_or(-120);
            if prev_portamento_end < next_portamento_start {
                interval_dict.insert(
                    prev_portamento_end as f64,
                    next_portamento_start as f64,
                    IntervalValue::Constant(prev_event.note_number as f64),
                );
            }
            let next_portamento_end = next_event.pos
                + next_note.portamento_offset.unwrap_or(-120)
                + next_note.portamento_length.unwrap_or(180);
            interval_dict.insert(
                next_portamento_start as f64,
                next_portamento_end as f64,
                IntervalValue::Interpolation {
                    x0: next_portamento_start as f64,
                    y0: prev_event.note_number as f64,
                    x1: next_portamento_end as f64,
                    y1: next_event.note_number as f64,
                },
            );
        } else if prev_portamento_end < prev_event.end_pos() {
            interval_dict.insert(
                prev_portamento_end as f64,
                prev_event.end_pos() as f64,
                IntervalValue::Constant(prev_event.note_number as f64),
            );
        }
        if index + 2 == count {
            let next_portamento_end = next_event.pos
                + next_note.portamento_offset.unwrap_or(-120)
                + next_note.portamento_length.unwrap_or(180);
            interval_dict.insert(
                next_portamento_end as f64,
                f64::INFINITY,
                IntervalValue::Constant(next_event.note_number as f64),
            );
        }
    }
    interval_dict
}
