//! Minimal ports of the `portion` helpers used by the acep plugin:
//! `RangeInterval.sub_ranges` (union of half-open ranges) and
//! `PiecewiseIntervalDict` (an interval -> value map with overwrite semantics).

use std::cmp::Ordering;

/// `libresvip.core.time_interval.RangeInterval`.
#[derive(Clone, Debug, Default)]
pub struct RangeInterval {
    ranges: Vec<(i64, i64)>,
}

impl RangeInterval {
    /// `RangeInterval([(start, end), ...])` — a union of `closedopen` intervals.
    pub fn new(ranges: &[(i64, i64)]) -> Self {
        let mut sorted: Vec<(i64, i64)> = ranges
            .iter()
            .copied()
            .filter(|(start, end)| start < end)
            .collect();
        sorted.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::with_capacity(sorted.len());
        for (start, end) in sorted {
            match merged.last_mut() {
                Some(last) if start <= last.1 => {
                    if end > last.1 {
                        last.1 = end;
                    }
                }
                _ => merged.push((start, end)),
            }
        }
        Self { ranges: merged }
    }

    /// `sub_ranges()` — the merged `(lower, upper)` bounds.
    pub fn sub_ranges(&self) -> Vec<(i64, i64)> {
        self.ranges.clone()
    }
}

#[derive(Clone, Debug)]
struct IntervalEntry<T> {
    lower: f64,
    lower_closed: bool,
    upper: f64,
    upper_closed: bool,
    value: T,
}

impl<T: Clone> IntervalEntry<T> {
    fn contains(&self, x: f64) -> bool {
        (x > self.lower || (self.lower_closed && x == self.lower))
            && (x < self.upper || (self.upper_closed && x == self.upper))
    }

    fn non_empty(&self) -> bool {
        self.lower < self.upper
            || (self.lower == self.upper && self.lower_closed && self.upper_closed)
    }

    /// Removes `[lower, upper]` from this entry, returning the remaining parts.
    fn subtract(
        &self,
        lower: f64,
        lower_closed: bool,
        upper: f64,
        upper_closed: bool,
    ) -> Vec<IntervalEntry<T>> {
        let touches = !(
            self.upper < lower
                || (self.upper == lower && !(self.upper_closed && lower_closed))
                || upper < self.lower
                || (upper == self.lower && !(upper_closed && self.lower_closed))
        );
        if !touches {
            return vec![self.clone()];
        }
        let mut parts = Vec::new();
        let left = IntervalEntry {
            lower: self.lower,
            lower_closed: self.lower_closed,
            upper: lower,
            upper_closed: !lower_closed,
            value: self.value.clone(),
        };
        if left.non_empty() {
            parts.push(left);
        }
        let right = IntervalEntry {
            lower: upper,
            lower_closed: !upper_closed,
            upper: self.upper,
            upper_closed: self.upper_closed,
            value: self.value.clone(),
        };
        if right.non_empty() {
            parts.push(right);
        }
        parts
    }
}

/// A faithful-enough `portion.IntervalDict`: later assignments override earlier
/// ones on the overlapping range.
#[derive(Clone, Debug)]
pub struct IntervalDict<T> {
    entries: Vec<IntervalEntry<T>>,
}

impl<T> Default for IntervalDict<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T: Clone> IntervalDict<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(
        &mut self,
        lower: f64,
        lower_closed: bool,
        upper: f64,
        upper_closed: bool,
        value: T,
    ) {
        let interval = IntervalEntry {
            lower,
            lower_closed,
            upper,
            upper_closed,
            value,
        };
        if !interval.non_empty() {
            return;
        }
        let mut remaining = Vec::with_capacity(self.entries.len() + 1);
        for entry in self.entries.drain(..) {
            remaining.extend(entry.subtract(lower, lower_closed, upper, upper_closed));
        }
        remaining.push(interval);
        remaining.sort_by(|left, right| {
            left.lower
                .partial_cmp(&right.lower)
                .unwrap_or(Ordering::Equal)
        });
        self.entries = remaining;
    }

    pub fn get(&self, x: f64) -> Option<T> {
        self.entries
            .iter()
            .find(|entry| entry.contains(x))
            .map(|entry| entry.value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_ranges_merges() {
        let interval = RangeInterval::new(&[(0, 10), (10, 20), (30, 40), (5, 5)]);
        assert_eq!(interval.sub_ranges(), vec![(0, 20), (30, 40)]);
    }

    #[test]
    fn interval_dict_overrides() {
        let mut dict: IntervalDict<i32> = IntervalDict::new();
        dict.set(0.0, true, 10.0, true, 1);
        dict.set(5.0, true, 15.0, true, 2);
        assert_eq!(dict.get(2.0), Some(1));
        assert_eq!(dict.get(5.0), Some(2));
        assert_eq!(dict.get(12.0), Some(2));
        assert_eq!(dict.get(16.0), None);
        assert_eq!(dict.get(10.0), Some(2));
    }
}
