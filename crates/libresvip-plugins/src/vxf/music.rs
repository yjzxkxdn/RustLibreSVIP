//! Numeric helpers for the VX-β plugin.

/// Python's `round(float)`.

pub use libresvip_core::core::compat::py_round;

/// Python's `round(value, digits)` — round-half-even on the exact binary value.
pub fn round_decimals(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}")
        .parse::<f64>()
        .unwrap_or(value)
}
