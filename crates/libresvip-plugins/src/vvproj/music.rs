//! Numeric helpers and the interval dictionary used by the VOICEVOX plugin.

/// `libresvip.utils.music_math.py_round` — Python's ties-to-even `round`.
pub fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// `libresvip.utils.music_math.hz2midi`.
pub fn hz2midi(hz: f64) -> f64 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// `libresvip.utils.music_math.midi2hz`.
pub fn midi2hz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}

/// `linear_interpolation(x, start, end)`.
pub fn linear_interpolation(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let ratio = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * ratio
}

/// `more_itertools.numeric_range(start, stop, step)`.
pub fn numeric_range(start: f64, stop: f64, step: f64) -> Vec<f64> {
    let mut values = Vec::new();
    if step == 0.0 {
        return values;
    }
    let mut index = 0i64;
    loop {
        let value = start + index as f64 * step;
        if step > 0.0 {
            if !(value < stop) {
                break;
            }
        } else if !(value > stop) {
            break;
        }
        values.push(value);
        index += 1;
    }
    values
}

#[derive(Clone, Debug)]
enum IntervalValue {
    Constant(f64),
    Linear { start: (f64, f64), end: (f64, f64) },
}

impl IntervalValue {
    fn evaluate(&self, x: f64) -> f64 {
        match *self {
            IntervalValue::Constant(value) => value,
            IntervalValue::Linear { start, end } => linear_interpolation(x, start, end),
        }
    }
}

#[derive(Clone, Debug)]
struct IntervalEntry {
    start: f64,
    end: f64,
    left_closed: bool,
    right_closed: bool,
    value: IntervalValue,
}

/// Port of `libresvip.core.time_interval.PiecewiseIntervalDict`: intervals may
/// overlap and lookups return the first match in `(lower, left-open)` order.
#[derive(Clone, Debug, Default)]
pub struct PiecewiseIntervalDict {
    entries: Vec<IntervalEntry>,
    sorted: bool,
}

impl PiecewiseIntervalDict {
    fn set(
        &mut self,
        start: f64,
        end: f64,
        left_closed: bool,
        right_closed: bool,
        value: IntervalValue,
    ) {
        // `portion.openclosed` is empty when start >= end; `singleton` never is.
        let empty = if left_closed && right_closed {
            start > end
        } else if left_closed {
            start > end
        } else {
            start >= end
        };
        if empty || start.is_nan() || end.is_nan() {
            return;
        }
        self.entries.push(IntervalEntry {
            start,
            end,
            left_closed,
            right_closed,
            value,
        });
        self.sorted = false;
    }

    pub fn set_openclosed_linear(
        &mut self,
        start: f64,
        end: f64,
        from: (f64, f64),
        to: (f64, f64),
    ) {
        self.set(
            start,
            end,
            false,
            true,
            IntervalValue::Linear { start: from, end: to },
        );
    }

    pub fn set_singleton(&mut self, point: f64, value: f64) {
        self.set(
            point,
            point,
            true,
            true,
            IntervalValue::Constant(value),
        );
    }

    fn ensure_sorted(&mut self) {
        if !self.sorted {
            // `SortedDict(_sortkey)` sorts by the lower bound, closed first.
            self.entries.sort_by(|left, right| {
                left.start
                    .partial_cmp(&right.start)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.left_closed.cmp(&right.left_closed).reverse())
            });
            self.sorted = true;
        }
    }

    /// `PiecewiseIntervalDict.get(x)`.
    pub fn get(&mut self, x: f64) -> Option<f64> {
        self.ensure_sorted();
        for entry in &self.entries {
            let left = if entry.left_closed {
                x >= entry.start
            } else {
                x > entry.start
            };
            let right = if entry.right_closed {
                x <= entry.end
            } else {
                x < entry.end
            };
            if left && right {
                return Some(entry.value.evaluate(x));
            }
        }
        None
    }

    /// `more_itertools.split_when(items, predicate)`, keeping every group.
    pub fn split_when<T: Copy>(items: &[T], predicate: impl Fn(T, T) -> bool) -> Vec<Vec<T>> {
        let mut groups: Vec<Vec<T>> = Vec::new();
        let mut iter = items.iter().copied();
        let Some(mut previous) = iter.next() else {
            return groups;
        };
        let mut group = vec![previous];
        for item in iter {
            if predicate(previous, item) {
                groups.push(std::mem::take(&mut group));
                group.push(item);
            } else {
                group.push(item);
            }
            previous = item;
        }
        groups.push(group);
        groups
    }
}
