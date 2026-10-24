//! Port of `libresvip.utils.search` and tiny `libresvip.core.compat` helpers.

/// `find_index`: first index satisfying `pred`, or `-1`.
pub fn find_index<T>(items: &[T], pred: impl Fn(&T) -> bool) -> isize {
    items
        .iter()
        .position(pred)
        .map(|index| index as isize)
        .unwrap_or(-1)
}

/// `find_last_index`: last index satisfying `pred`, or `-1`.
pub fn find_last_index<T>(items: &[T], pred: impl Fn(&T) -> bool) -> isize {
    items
        .iter()
        .rposition(pred)
        .map(|index| index as isize)
        .unwrap_or(-1)
}

/// `binary_find_first` (assumes `pred` is monotone: false…false,true…true).
pub fn binary_find_first<T: Copy>(items: &[T], pred: impl Fn(&T) -> bool) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    let (mut left, mut right) = (0usize, items.len() - 1);
    while right > left {
        let middle = (left + right) / 2;
        if pred(&items[middle]) {
            right = middle;
        } else {
            left = middle + 1;
        }
    }
    if pred(&items[right]) {
        Some(items[right])
    } else {
        None
    }
}

/// `binary_find_last` (assumes `pred` is monotone: true…true,false…false).
pub fn binary_find_last<T: Copy>(items: &[T], pred: impl Fn(&T) -> bool) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    let (mut left, mut right) = (0usize, items.len() - 1);
    while right > left {
        let middle = (left + right + 1) / 2;
        if pred(&items[middle]) {
            left = middle;
        } else {
            right = middle - 1;
        }
    }
    if pred(&items[left]) {
        Some(items[left])
    } else {
        None
    }
}

/// Python's `math.ceil` for floats, returning `i64`.
pub fn ceil_i64(value: f64) -> i64 {
    value.ceil() as i64
}

/// Python's `round()` for floats, returning `i64` (banker's rounding).
///
/// Unified semantics for the whole workspace (previously 20 per-plugin copies
/// with divergent non-finite handling): the saturating `as` cast — `NaN → 0`,
/// `±inf → ±i64::MAX`. Python itself raises on non-finite input, so any value
/// relying on the saturation branches is already corrupt data.
#[inline]
pub fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}
