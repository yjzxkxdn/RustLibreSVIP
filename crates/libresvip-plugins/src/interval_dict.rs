//! Shared generic interval dictionary — the storage/matching core behind the
//! per-plugin `PiecewiseIntervalDict` ports of `portion.IntervalDict`.
//!
//! The Python plugins layer `portion` intervals into a `SortedDict`; each Rust
//! port used to carry its own copy of the entry list, empty-interval rule,
//! sorting and first-match lookup. This module keeps the behaviour of those
//! copies exactly:
//!
//! * stable sort by lower bound (`partial_cmp`, `NaN` compares equal); with
//!   `closed_first` the closed-left entry wins on equal bounds;
//! * [`IntervalDict::get`] returns the first entry in that order whose bounds
//!   contain `x` — overlapping intervals are allowed and the earliest wins;
//! * an interval is empty when `start > end`, or when `start == end` unless
//!   both bounds are closed (so a closed singleton survives);
//! * `strict_nan` additionally drops intervals with `NaN` bounds, mirroring
//!   the copies that had an explicit `is_nan` guard.

/// One interval plus its plugin-defined value.
#[derive(Clone, Debug)]
pub struct IntervalEntry<V> {
    pub start: f64,
    pub end: f64,
    pub start_closed: bool,
    pub end_closed: bool,
    pub value: V,
}

/// `portion.IntervalDict` + `SortedDict` core, parameterised over the value.
#[derive(Clone, Debug)]
pub struct IntervalDict<V> {
    entries: Vec<IntervalEntry<V>>,
    sorted: bool,
    strict_nan: bool,
    closed_first: bool,
    /// `Some(true)` when the sorted entries do not overlap, which allows the
    /// lookup to binary-search instead of scanning every interval.
    non_overlapping: Option<bool>,
}

impl<V> Default for IntervalDict<V> {
    /// Lenient policy (no `NaN` guard, ties in insertion order).
    fn default() -> Self {
        Self::new(false, false)
    }
}

impl<V> IntervalDict<V> {
    pub fn new(strict_nan: bool, closed_first: bool) -> Self {
        Self {
            entries: Vec::new(),
            sorted: true,
            strict_nan,
            closed_first,
            non_overlapping: Some(true),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Inserts one interval, dropping it when empty per the module rules.
    pub fn set(
        &mut self,
        start: f64,
        end: f64,
        start_closed: bool,
        end_closed: bool,
        value: V,
    ) {
        let empty = start > end || (start == end && !(start_closed && end_closed));
        if empty || (self.strict_nan && (start.is_nan() || end.is_nan())) {
            return;
        }
        self.entries.push(IntervalEntry {
            start,
            end,
            start_closed,
            end_closed,
            value,
        });
        self.sorted = false;
        self.non_overlapping = None;
    }

    fn ensure_sorted(&mut self) {
        if !self.sorted {
            let closed_first = self.closed_first;
            self.entries.sort_by(|left, right| {
                left.start
                    .partial_cmp(&right.start)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        if closed_first {
                            left.start_closed.cmp(&right.start_closed).reverse()
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
            });
            self.sorted = true;
            self.non_overlapping = None;
        }
    }

    /// Sorted and disjoint, so the candidate interval can be found by binary
    /// search. Overlapping dictionaries (allowed by the `portion` semantics)
    /// keep the linear first-match scan.
    fn is_non_overlapping(&mut self) -> bool {
        if let Some(known) = self.non_overlapping {
            return known;
        }
        let disjoint = self.entries.windows(2).all(|pair| {
            let (previous, next) = (&pair[0], &pair[1]);
            next.start > previous.end
                || (next.start == previous.end && !(next.start_closed && previous.end_closed))
        });
        self.non_overlapping = Some(disjoint);
        disjoint
    }

    /// Value of the first interval (in sorted order) containing `x`.
    pub fn get(&mut self, x: f64, eval: impl Fn(&V, f64) -> f64) -> Option<f64> {
        self.ensure_sorted();
        if self.is_non_overlapping() {
            // With disjoint intervals at most one entry can contain `x`, and it
            // must be the last one whose start does not exceed it.
            let index = self
                .entries
                .partition_point(|entry| {
                    if entry.start_closed {
                        entry.start <= x
                    } else {
                        entry.start < x
                    }
                });
            let entry = self.entries.get(index.checked_sub(1)?)?;
            let after_start = if entry.start_closed {
                x >= entry.start
            } else {
                x > entry.start
            };
            let before_end = if entry.end_closed {
                x <= entry.end
            } else {
                x < entry.end
            };
            return (after_start && before_end).then(|| eval(&entry.value, x));
        }
        for entry in &self.entries {
            let after_start = if entry.start_closed {
                x >= entry.start
            } else {
                x > entry.start
            };
            let before_end = if entry.end_closed {
                x <= entry.end
            } else {
                x < entry.end
            };
            if after_start && before_end {
                return Some(eval(&entry.value, x));
            }
        }
        None
    }
}
