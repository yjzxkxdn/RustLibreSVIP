//! Numeric helpers shared by the TuneLab parser and generator.

use std::f64::consts::PI;

/// `libresvip.utils.music_math.db_to_float` (amplitude flavour).
pub fn db_to_float(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// `libresvip.utils.music_math.ratio_to_db` (amplitude flavour).
pub fn ratio_to_db(ratio: f64) -> f64 {
    if ratio == 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * ratio.log10()
}

/// Python's `round(float)`.

pub use libresvip_core::core::compat::py_round;

/// `linear_interpolation(x, start, end)`.
pub fn linear_interpolation(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let ratio = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * ratio
}

/// `more_itertools.numeric_range(start, stop, step)`: `start + n * step` while
/// the value stays below `stop` (accumulation-free, like the reference class).
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

fn f1(t1: f64, t2: f64) -> f64 {
    (1.0 + 2.0 * t1) * t2.powi(2)
}

fn f3(t: f64, d: f64) -> f64 {
    t.powi(2) * d
}

fn slope(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    if p2.0 != p1.0 {
        (p2.1 - p1.1) / (p2.0 - p1.0)
    } else {
        f64::NAN
    }
}

/// Port of `libresvip.utils.music_math.HermiteInterpolator`.
#[derive(Clone, Debug)]
pub struct HermiteInterpolator {
    points: Vec<(f64, f64)>,
}

impl HermiteInterpolator {
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        Self { points }
    }

    fn slope_at(&self, point_index: usize) -> f64 {
        let last_index = self.points.len() - 1;
        if point_index == 0 || point_index == last_index {
            return 0.0;
        }
        let point = self.points[point_index];
        let last_k = slope(point, self.points[point_index - 1]);
        let next_k = slope(point, self.points[point_index + 1]);
        let kk = last_k * next_k;
        if kk <= 0.0 {
            0.0
        } else {
            2.0 / (1.0 / last_k + 1.0 / next_k)
        }
    }

    pub fn interpolate(&self, xs: &[f64]) -> Vec<f64> {
        if self.points.len() < 2 {
            let value = if self.points.len() == 1 {
                self.points[0].1
            } else {
                0.0
            };
            return vec![value; xs.len()];
        }
        if self.points.len() == 2 {
            return xs
                .iter()
                .map(|&x| linear_interpolation(x, self.points[0], self.points[1]))
                .collect();
        }
        let mut point_index = 0usize;
        let mut last_delta_index: isize = -1;
        let mut last_delta = 0.0f64;
        let mut ys = Vec::with_capacity(xs.len());
        for &x in xs {
            while point_index < self.points.len() && self.points[point_index].0 < x {
                point_index += 1;
            }
            if point_index == 0 {
                ys.push(self.points[0].1);
            } else if point_index == self.points.len() {
                ys.push(self.points[self.points.len() - 1].1);
            } else {
                let last_point = self.points[point_index - 1];
                if point_index as isize != last_delta_index {
                    last_delta = self.slope_at(point_index - 1);
                    last_delta_index = point_index as isize;
                }
                let next_point = self.points[point_index];
                let next_delta = self.slope_at(point_index);
                let delta_1 = x - last_point.0;
                let delta_2 = x - next_point.0;
                let t1 = delta_1 / (next_point.0 - last_point.0);
                let t2 = delta_2 / (last_point.0 - next_point.0);
                ys.push(
                    f1(t1, t2) * last_point.1
                        + f1(t2, t1) * next_point.1
                        + f3(t2, delta_1) * last_delta
                        + f3(t1, delta_2) * next_delta,
                );
            }
        }
        ys
    }
}

/// `TuneLabParser.vibrato_value`.
pub fn vibrato_value(seconds: f64, vibrato_start: f64, frequency: f64, amplitude: f64, phase: f64) -> f64 {
    (PI * (2.0 * (seconds - vibrato_start) * frequency - phase)).sin() * amplitude
}

#[derive(Clone, Debug)]
enum IntervalValue {
    Constant(f64),
    Vibrato {
        start: f64,
        frequency: f64,
        amplitude: f64,
        phase: f64,
    },
}

impl IntervalValue {
    fn evaluate(&self, x: f64) -> f64 {
        match *self {
            IntervalValue::Constant(value) => value,
            IntervalValue::Vibrato {
                start,
                frequency,
                amplitude,
                phase,
            } => vibrato_value(x, start, frequency, amplitude, phase),
        }
    }
}

/// Port of `libresvip.core.time_interval.PiecewiseIntervalDict` restricted to
/// the closed / closed-open intervals the TuneLab parser uses. Overlapping
/// intervals are allowed and the first one (sorted by lower bound) wins.
/// Storage/matching lives in the shared `IntervalDict` (strict `NaN` guard).
#[derive(Clone, Debug)]
pub struct PiecewiseIntervalDict {
    dict: crate::interval_dict::IntervalDict<IntervalValue>,
}

impl Default for PiecewiseIntervalDict {
    fn default() -> Self {
        Self {
            dict: crate::interval_dict::IntervalDict::new(true, false),
        }
    }
}

impl PiecewiseIntervalDict {

    pub fn set_closed_vibrato(
        &mut self,
        start: f64,
        end: f64,
        vibrato_start: f64,
        frequency: f64,
        amplitude: f64,
        phase: f64,
    ) {
        self.dict.set(
            start,
            end,
            true,
            true,
            IntervalValue::Vibrato {
                start: vibrato_start,
                frequency,
                amplitude,
                phase,
            },
        );
    }

    pub fn set_closedopen_constant(&mut self, start: f64, end: f64, value: f64) {
        self.dict.set(start, end, true, false, IntervalValue::Constant(value));
    }

    /// `PiecewiseIntervalDict.get(x)`.
    pub fn get(&mut self, x: f64) -> Option<f64> {
        self.dict.get(x, |value, x| value.evaluate(x))
    }
}
