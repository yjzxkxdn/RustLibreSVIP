//! Numeric, path and audio helpers shared by the DSPX parser and generator.

use std::path::{Component, Path, PathBuf};

/// `libresvip.utils.music_math.clamp`.
pub fn clamp(value: f64, lower: f64, upper: f64) -> f64 {
    if upper < lower {
        return value;
    }
    value.max(lower).min(upper)
}

/// `libresvip.utils.music_math.db_to_float`.
pub fn db_to_float(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// `libresvip.utils.music_math.ratio_to_db`.
pub fn ratio_to_db(ratio: f64) -> f64 {
    if ratio == 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * ratio.log10()
}

// Python's `round(float)`.
pub use libresvip_core::core::compat::py_round;

pub fn ceil_to_interval(value: i64) -> i64 {
    let quotient = value.div_euclid(5);
    let remainder = value.rem_euclid(5);
    (quotient + i64::from(remainder != 0)) * 5
}

pub fn floor_to_interval(value: i64) -> i64 {
    value.div_euclid(5) * 5
}

pub fn round_to_interval(value: i64) -> i64 {
    py_round(value as f64 / 5.0) * 5
}

/// Python's floor division for integers (the result rounds towards −∞).
pub fn floordiv_i64(a: i64, b: i64) -> i64 {
    if b == 0 {
        return 0;
    }
    a.div_euclid(b)
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// `Path.resolve()` for a possibly non-existent relative path.
pub fn resolve_path(relative: &str, base: &Path) -> String {
    let path = Path::new(relative);
    if path.is_absolute() {
        return relative.to_owned();
    }
    let joined = base.join(path);
    let absolute = if joined.is_absolute() {
        joined
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(&joined))
            .unwrap_or(joined)
    };
    normalize_path(&absolute).to_string_lossy().into_owned()
}
