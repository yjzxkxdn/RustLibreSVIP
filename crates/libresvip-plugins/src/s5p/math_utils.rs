//! Small numeric helpers shared by the s5p parser and generator.
//!
//! These mirror the subset of `libresvip.utils.music_math` and the Python
//! builtins that the plugin relies on.

/// Python's `round()`: ties to even, returning an integer.
#[inline]
pub fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
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

/// `libresvip.utils.music_math.linear_interpolation` over integer points.
pub fn linear_interpolation(x: i64, start: (i64, i64), end: (i64, i64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    if x1 == x0 {
        return y0 as f64;
    }
    let r = (x - x0) as f64 / (x1 - x0) as f64;
    y0 as f64 + (y1 - y0) as f64 * r
}
