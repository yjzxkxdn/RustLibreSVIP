//! Small numeric helpers shared by the SVP parser and generator.

use crate::svp::constants::TICK_RATE;

/// `libresvip.plugins.svp.interval_utils.position_to_ticks`
pub fn position_to_ticks(position: i64) -> i64 {
    py_round(position as f64 / TICK_RATE as f64)
}

/// `libresvip.plugins.svp.interval_utils.ticks_to_position`
pub fn ticks_to_position(ticks: i64) -> i64 {
    ticks * TICK_RATE
}

/// Python's `round()`: ties to even.

pub use libresvip_core::core::compat::py_round;

/// `libresvip.utils.music_math.clamp`.
#[inline]
pub fn clamp(value: f64, lower: f64, upper: f64) -> f64 {
    value.min(upper).max(lower)
}

/// `libresvip.utils.music_math.db_to_float`.
#[inline]
pub fn db_to_float(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// `libresvip.utils.music_math.ratio_to_db`.
#[inline]
pub fn ratio_to_db(ratio: f64) -> f64 {
    if ratio == 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * ratio.log10()
    }
}

/// The three interpolation modes the SVP parser recognises. Anything else
/// (including `"sigmoid"` and `"linear"`) falls back to linear, exactly like
/// `SynthVParser.parse_interpolation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Cosine,
    Cubic,
}

impl Interpolation {
    pub fn from_mode(mode: &str) -> Self {
        match mode {
            "cosine" => Interpolation::Cosine,
            "cubic" => Interpolation::Cubic,
            _ => Interpolation::Linear,
        }
    }

    /// `f(x, (x0, y0), (x1, y1))`.
    pub fn interpolate(self, x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
        let (x0, y0) = start;
        let (x1, y1) = end;
        let r = if x1 == x0 { 0.0 } else { (x - x0) / (x1 - x0) };
        let factor = match self {
            Interpolation::Linear => r,
            Interpolation::Cosine => (1.0 - (r * std::f64::consts::PI).cos()) / 2.0,
            Interpolation::Cubic => (3.0 - 2.0 * r) * r * r,
        };
        y0 + (y1 - y0) * factor
    }
}

/// A set of half-open integer ranges, mirroring the subset of `portion` used
/// by `RangeInterval` (`closedopen` sub-ranges, union, intersection, shift and
/// expansion).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RangeInterval {
    ranges: Vec<(i64, i64)>,
}

impl RangeInterval {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_pairs<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (i64, i64)>,
    {
        let mut result = Self::new();
        for (start, end) in pairs {
            result.insert(start, end);
        }
        result
    }

    fn insert(&mut self, start: i64, end: i64) {
        if start >= end {
            return;
        }
        self.ranges.push((start, end));
        self.normalize();
    }

    fn normalize(&mut self) {
        self.ranges.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::with_capacity(self.ranges.len());
        for (start, end) in self.ranges.drain(..) {
            if let Some(last) = merged.last_mut() {
                if start <= last.1 {
                    if end > last.1 {
                        last.1 = end;
                    }
                    continue;
                }
            }
            merged.push((start, end));
        }
        self.ranges = merged;
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn includes(&self, value: i64) -> bool {
        self.ranges
            .iter()
            .any(|(start, end)| *start <= value && value < *end)
    }

    pub fn sub_ranges(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        self.ranges.iter().copied()
    }

    pub fn union(&self, other: &RangeInterval) -> RangeInterval {
        let mut result = self.clone();
        for (start, end) in other.ranges.iter().copied() {
            result.insert(start, end);
        }
        result
    }

    pub fn intersection(&self, other: &RangeInterval) -> RangeInterval {
        let mut ranges = Vec::new();
        for (a_start, a_end) in self.ranges.iter().copied() {
            for (b_start, b_end) in other.ranges.iter().copied() {
                let start = a_start.max(b_start);
                let end = a_end.min(b_end);
                if start < end {
                    ranges.push((start, end));
                }
            }
        }
        let mut result = RangeInterval { ranges };
        result.normalize();
        result
    }

    pub fn expand(&self, radius: i64) -> RangeInterval {
        RangeInterval::from_pairs(
            self.ranges
                .iter()
                .map(|(start, end)| (start - radius, end + radius)),
        )
    }

    pub fn shrink(&self, radius: i64) -> RangeInterval {
        self.expand(-radius)
    }

    pub fn shift(&self, offset: i64) -> RangeInterval {
        RangeInterval::from_pairs(
            self.ranges
                .iter()
                .map(|(start, end)| (start + offset, end + offset)),
        )
    }

    /// Union in place (`|=`).
    pub fn union_with(&mut self, other: &RangeInterval) {
        for (start, end) in other.ranges.iter().copied() {
            self.insert(start, end);
        }
    }

    /// Intersection in place (`&=`).
    pub fn intersect_with(&mut self, other: &RangeInterval) {
        *self = self.intersection(other);
    }
}

/// `more_itertools.locate`-style first index, or `-1`.
pub fn find_index<T>(items: &[T], pred: impl Fn(&T) -> bool) -> isize {
    items
        .iter()
        .position(pred)
        .map(|index| index as isize)
        .unwrap_or(-1)
}

/// `more_itertools.rlocate`-style last index, or `-1`.
pub fn find_last_index<T>(items: &[T], pred: impl Fn(&T) -> bool) -> isize {
    items
        .iter()
        .rposition(pred)
        .map(|index| index as isize)
        .unwrap_or(-1)
}

/// `bisect.bisect_right` (same binary search as CPython, including its
/// behaviour on unsorted input).
pub fn bisect_right<T: PartialOrd>(items: &[T], value: &T) -> usize {
    let mut low = 0usize;
    let mut high = items.len();
    while low < high {
        let middle = (low + high) / 2;
        if *value < items[middle] {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

/// `bisect.bisect_left` (same binary search as CPython).
pub fn bisect_left<T: PartialOrd>(items: &[T], value: &T) -> usize {
    let mut low = 0usize;
    let mut high = items.len();
    while low < high {
        let middle = (low + high) / 2;
        if items[middle] < *value {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

/// `bisect.bisect_right` for floats.
pub fn bisect_right_f64(items: &[f64], value: f64) -> usize {
    bisect_right(items, &value)
}

/// `bisect.bisect_left` for floats.
pub fn bisect_left_f64(items: &[f64], value: f64) -> usize {
    bisect_left(items, &value)
}

/// `libresvip.utils.search.binary_find_last` over `(position, index)` pairs.
pub fn binary_find_last_index(items: &[(i64, usize)], ticks: i64) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let (mut left, mut right) = (0usize, items.len() - 1);
    while right > left {
        let middle = (left + right + 1) / 2;
        if items[middle].0 <= ticks {
            left = middle;
        } else {
            right = middle - 1;
        }
    }
    if items[left].0 <= ticks {
        Some(items[left].1)
    } else {
        None
    }
}
