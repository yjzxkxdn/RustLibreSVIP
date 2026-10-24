//! Numeric helpers and the interval dictionary used by the VOICEVOX plugin.

/// `libresvip.utils.music_math.py_round` — Python's ties-to-even `round`.

pub use libresvip_core::core::compat::py_round;

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

/// Port of `libresvip.core.time_interval.PiecewiseIntervalDict`: intervals may
/// overlap and lookups return the first match in `(lower, left-open)` order.
/// Storage/matching lives in the shared `IntervalDict` (strict `NaN` guard,
/// closed-left entries first on equal lower bounds).
#[derive(Clone, Debug)]
pub struct PiecewiseIntervalDict {
    dict: crate::interval_dict::IntervalDict<IntervalValue>,
}

impl Default for PiecewiseIntervalDict {
    fn default() -> Self {
        Self {
            dict: crate::interval_dict::IntervalDict::new(true, true),
        }
    }
}

impl PiecewiseIntervalDict {
    pub fn set_openclosed_linear(
        &mut self,
        start: f64,
        end: f64,
        from: (f64, f64),
        to: (f64, f64),
    ) {
        self.dict.set(
            start,
            end,
            false,
            true,
            IntervalValue::Linear { start: from, end: to },
        );
    }

    pub fn set_singleton(&mut self, point: f64, value: f64) {
        self.dict.set(
            point,
            point,
            true,
            true,
            IntervalValue::Constant(value),
        );
    }

    /// `PiecewiseIntervalDict.get(x)`.
    pub fn get(&mut self, x: f64) -> Option<f64> {
        self.dict.get(x, |value, x| value.evaluate(x))
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
