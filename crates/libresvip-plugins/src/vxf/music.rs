//! Numeric helpers for the VX-β plugin.

/// Python's `round(float)`.
pub fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// Python's `round(value, digits)` — round-half-even on the exact binary value.
pub fn round_decimals(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}")
        .parse::<f64>()
        .unwrap_or(value)
}
