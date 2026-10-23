//! Port of the `libresvip.utils.music_math` helpers the DiffSinger plugin uses.

use libresvip_core::error::{LibreSvipError, Result};

pub const KEY_IN_OCTAVE: i64 = 12;

const PITCH_MAP: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Python's `round(float)` (banker's rounding).
#[inline]
pub fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// Python's `round(value, ndigits)` for a positive digit count.
#[inline]
pub fn round_ndigits(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}").parse::<f64>().unwrap_or(value)
}

/// `midi2note` — Python's `round` rounds ties to even and `//` floors.
pub fn midi2note(midi: f64) -> String {
    let midi = py_round(midi);
    let octave = midi.div_euclid(KEY_IN_OCTAVE) - 1;
    let pitch = PITCH_MAP[midi.rem_euclid(KEY_IN_OCTAVE) as usize];
    format!("{pitch}{octave}")
}

fn accidental_offset(character: char) -> Option<i64> {
    match character {
        '#' | '♯' => Some(1),
        '𝄪' => Some(2),
        'b' | '!' | '♭' => Some(-1),
        '𝄫' => Some(-2),
        '♮' => Some(0),
        _ => None,
    }
}

fn pitch_offset(character: char) -> Option<i64> {
    match character.to_ascii_uppercase() {
        'C' => Some(0),
        'D' => Some(2),
        'E' => Some(4),
        'F' => Some(5),
        'G' => Some(7),
        'A' => Some(9),
        'B' => Some(11),
        _ => None,
    }
}

/// `note2midi`.
///
/// The Python regex is `^(?P<note>[A-Ga-g])(?P<accidental>[#♯𝄪b!♭𝄫♮]*)(?P<octave>[+-]?\d+)?$`.
pub fn note2midi(note: &str) -> Result<i64> {
    let mut characters = note.chars().peekable();
    let invalid = || LibreSvipError::msg(format!("Invalid note format: {note:?}"));
    let Some(step) = characters.next() else {
        return Err(invalid());
    };
    let Some(pitch) = pitch_offset(step) else {
        return Err(invalid());
    };
    let mut offset = 0i64;
    while let Some(&character) = characters.peek() {
        let Some(accidental) = accidental_offset(character) else {
            break;
        };
        offset += accidental;
        characters.next();
    }
    let rest: String = characters.collect();
    let octave = if rest.is_empty() { 0 } else { rest.parse::<i64>().map_err(|_| invalid())? };
    Ok(py_round(
        (KEY_IN_OCTAVE * (octave + 1) + pitch + offset) as f64,
    ))
}

/// `hz2midi`.
pub fn hz2midi(hz: f64) -> Result<f64> {
    if !(hz > 0.0) || !hz.is_finite() {
        // `math.log2` raises `ValueError: math domain error` for these.
        return Err(LibreSvipError::msg("math domain error"));
    }
    Ok(69.0 + KEY_IN_OCTAVE as f64 * (hz / 440.0).log2())
}

/// `midi2hz`.
#[inline]
pub fn midi2hz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / KEY_IN_OCTAVE as f64)
}

/// Python's `str(float)` / `repr(float)`.
///
/// Rust's `Display` never switches to exponent notation, whereas CPython uses
/// scientific notation for exponents below -4 or at least 16, so the shortest
/// round-trip digits are re-laid-out here.
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_owned()
        } else {
            "0.0".to_owned()
        };
    }
    let negative = value < 0.0;
    let scientific = format!("{:e}", value.abs());
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return value.to_string();
    };
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if negative { "-" } else { "" };
    if !(-4..16).contains(&exponent) {
        let mut out = String::new();
        out.push_str(sign);
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        if exponent < 0 {
            out.push('-');
        } else {
            out.push('+');
        }
        let magnitude = exponent.unsigned_abs();
        if magnitude < 10 {
            out.push('0');
        }
        out.push_str(&magnitude.to_string());
        return out;
    }
    let point = exponent + 1;
    let mut out = String::new();
    out.push_str(sign);
    if point <= 0 {
        out.push_str("0.");
        for _ in 0..(-point) {
            out.push('0');
        }
        out.push_str(digits);
    } else if (point as usize) >= digits.len() {
        out.push_str(digits);
        for _ in 0..(point as usize - digits.len()) {
            out.push('0');
        }
        out.push_str(".0");
    } else {
        out.push_str(&digits[..point as usize]);
        out.push('.');
        out.push_str(&digits[point as usize..]);
    }
    out
}
