//! Port of `libresvip.utils.music_math` helpers used by the VSPX plugin.

use std::sync::OnceLock;

use regex::Regex;

/// `KEY_IN_OCTAVE`.
const KEY_IN_OCTAVE: i64 = 12;

const NOTE2MIDI_PITCH_MAP: [(char, i64); 7] = [
    ('C', 0),
    ('D', 2),
    ('E', 4),
    ('F', 5),
    ('G', 7),
    ('A', 9),
    ('B', 11),
];

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

fn note_regex() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^([A-Ga-g])([#♯𝄪b!♭𝄫♮]*)([+-]?\d+)?$").expect("static note regex")
    })
}

/// `midi2note(midi)`.
pub fn midi2note(midi: f64) -> String {
    const PITCH_MAP: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let midi = midi.round_ties_even() as i64;
    let octave = midi.div_euclid(KEY_IN_OCTAVE) - 1;
    let pitch = PITCH_MAP[midi.rem_euclid(KEY_IN_OCTAVE) as usize];
    format!("{pitch}{octave}")
}

/// `note2midi(note)`.
pub fn note2midi(note: &str) -> Result<i64, String> {
    let Some(captures) = note_regex().captures(note) else {
        return Err(format!("Invalid note format: {note:?}"));
    };
    let pitch = captures
        .get(1)
        .map(|matched| matched.as_str())
        .unwrap_or_default()
        .chars()
        .next()
        .unwrap_or('C')
        .to_ascii_uppercase();
    let pitch_offset = NOTE2MIDI_PITCH_MAP
        .iter()
        .find(|(name, _)| *name == pitch)
        .map(|(_, offset)| *offset)
        .unwrap_or(0);
    let mut offset = 0i64;
    if let Some(accidentals) = captures.get(2) {
        for character in accidentals.as_str().chars() {
            offset += accidental_offset(character).unwrap_or(0);
        }
    }
    let octave = captures
        .get(3)
        .map(|matched| matched.as_str())
        .filter(|text| !text.is_empty())
        .map(|text| text.parse::<i64>().unwrap_or(0))
        .unwrap_or(0);
    let note_value = KEY_IN_OCTAVE * (octave + 1) + pitch_offset + offset;
    Ok(note_value)
}

/// `linear_interpolation(x, start, end)`.
pub fn linear_interpolation(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let ratio = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * ratio
}

/// `cosine_easing_in_out_interpolation(x, start, end)`.
pub fn cosine_easing_in_out_interpolation(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let ratio = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * (1.0 - (ratio * std::f64::consts::PI).cos()) / 2.0
}

/// `clamp(x, lower, upper)`.
pub fn clamp(value: f64, lower: f64, upper: f64) -> f64 {
    value.max(lower).min(upper)
}
