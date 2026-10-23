//! Port of the pieces of `libresvip.utils.music_math` the acep plugin uses,
//! plus the exact floating point sums the Python reference relies on.

/// `libresvip.utils.music_math.clamp`.
pub fn clamp(value: f64, lower: Option<f64>, upper: Option<f64>) -> f64 {
    let lower = lower.unwrap_or(f64::NEG_INFINITY);
    let upper = upper.unwrap_or(f64::INFINITY);
    value.max(lower).min(upper)
}

/// CPython 3.12's `sum()` over floats: Neumaier (compensated) summation.
///
/// The Python reference computes the raised-cosine kernel sum with `sum()` and
/// the normalisation statistics with `statistics`, both of which are
/// compensated since 3.12, so a naive `fold` would drift by an ulp or two.
pub fn neumaier_sum(values: &[f64]) -> f64 {
    let mut sum = 0.0f64;
    let mut compensation = 0.0f64;
    for &value in values {
        let total = sum + value;
        if sum.abs() >= value.abs() {
            compensation += (sum - total) + value;
        } else {
            compensation += (value - total) + sum;
        }
        sum = total;
    }
    sum + compensation
}

/// `math.fsum` — Shewchuk's expansion of non-overlapping partials plus CPython's
/// half-even fixup, i.e. the correctly rounded exact sum.
#[derive(Clone, Debug, Default)]
pub struct ExactSum {
    partials: Vec<f64>,
}

impl ExactSum {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, mut value: f64) {
        let mut index = 0usize;
        for position in 0..self.partials.len() {
            let mut partial = self.partials[position];
            if value.abs() < partial.abs() {
                std::mem::swap(&mut value, &mut partial);
            }
            let hi = value + partial;
            let lo = partial - (hi - value);
            if lo != 0.0 {
                self.partials[index] = lo;
                index += 1;
            }
            value = hi;
        }
        self.partials.truncate(index);
        self.partials.push(value);
    }

    pub fn value(&self) -> f64 {
        if self.partials.is_empty() {
            return 0.0;
        }
        let mut n = self.partials.len() - 1;
        let mut hi = self.partials[n];
        let mut lo = 0.0f64;
        while n > 0 {
            let x = hi;
            let y = self.partials[n - 1];
            hi = x + y;
            let yr = hi - x;
            lo = y - yr;
            if lo != 0.0 {
                break;
            }
            n -= 1;
        }
        if n > 0
            && ((lo < 0.0 && self.partials[n - 1] < 0.0)
                || (lo > 0.0 && self.partials[n - 1] > 0.0))
        {
            let y = lo * 2.0;
            let x = hi + y;
            let yr = x - hi;
            if y == yr {
                hi = x;
            }
        }
        hi
    }
}

/// `math.fsum(values)`.
pub fn exact_sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut sum = ExactSum::new();
    for value in values {
        sum.add(value);
    }
    sum.value()
}

/// `DoubleLength` / `TripleLength` and their error-free transformations, from
/// CPython's `Modules/mathmodule.c` (Ogita/Rump/Oishi).
#[derive(Clone, Copy)]
struct DoubleLength {
    hi: f64,
    lo: f64,
}

/// `dl_sum` — Algorithm 3.1, `a + b` as a double-length number.
fn dl_sum(a: f64, b: f64) -> DoubleLength {
    let x = a + b;
    let z = x - a;
    let y = (a - (x - z)) + (b - z);
    DoubleLength { hi: x, lo: y }
}

/// `dl_mul` — Algorithm 3.5, `x * y` as a double-length number.
fn dl_mul(x: f64, y: f64) -> DoubleLength {
    let z = x * y;
    let zz = x.mul_add(y, -z);
    DoubleLength { hi: z, lo: zz }
}

#[derive(Clone, Copy, Default)]
struct TripleLength {
    hi: f64,
    lo: f64,
    tiny: f64,
}

/// `tl_fma` — Algorithm 5.10 with SumKVert for K=3, i.e. `total + x * y`.
fn tl_fma(x: f64, y: f64, total: TripleLength) -> TripleLength {
    let pr = dl_mul(x, y);
    let sm = dl_sum(total.hi, pr.hi);
    let r1 = dl_sum(total.lo, pr.lo);
    let r2 = dl_sum(r1.hi, sm.lo);
    TripleLength {
        hi: sm.hi,
        lo: r2.hi,
        tiny: total.tiny + r1.lo + r2.lo,
    }
}

/// `tl_to_d` — the correctly rounded double of a triple-length number.
fn tl_to_d(total: TripleLength) -> f64 {
    let last = dl_sum(total.lo, total.hi);
    total.tiny + last.lo + last.hi
}

/// `math.sumprod(left, right)`.
///
/// CPython 3.12 accumulates the products in a [`TripleLength`] (≈ 150 bits) and
/// rounds once at the end, so this is both the reference behaviour and O(n) in
/// constant space — unlike [`exact_sum`], whose Shewchuk expansion costs
/// O(n · partials) per call.
pub fn exact_sumprod(left: &[f64], right: &[f64]) -> f64 {
    let mut total = TripleLength::default();
    for (x, y) in left.iter().zip(right.iter()) {
        total = tl_fma(*x, *y, total);
    }
    tl_to_d(total)
}

/// `linear_interpolation(x, start, end)`.
pub fn linear_interpolation(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let ratio = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * ratio
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values were produced by `math.sumprod` (CPython 3.12).
    #[test]
    fn sumprod_matches_cpython() {
        let cases: [(&[f64], &[f64], f64); 4] = [
            (&[1.0, 1e16, -1e16, 1.0], &[1.0, 1.0, 1.0, 1.0], 2.0),
            (&[0.1, 0.2, 0.3], &[1e-3, 2e-3, 3e-3], 0.0014),
            (
                &[1.0 / 3.0, 1.0 / 7.0, 1.0 / 9.0],
                &[3.14159, 2.71828, 1.41421],
                1.5926568253968252,
            ),
            (&[0.5; 119], &[1.0 / 119.0; 119], 0.5),
        ];
        for (left, right, expected) in cases {
            assert_eq!(exact_sumprod(left, right).to_bits(), expected.to_bits());
        }
    }

    /// The compensated sums are only useful if they beat a naive `fold`.
    #[test]
    fn compensated_sums_beat_naive_summation() {
        let values = [1.0f64, 1e100, 1.0, -1e100];
        let naive: f64 = values.iter().sum();
        assert_eq!(naive, 0.0);
        assert_eq!(exact_sum(values.iter().copied()), 2.0);
    }
}
