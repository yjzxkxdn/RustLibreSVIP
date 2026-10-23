//! Music theory helpers used by both the MusicXML parser and generator
//! (`libresvip.utils.music_math`).

pub const KEY_IN_OCTAVE: i64 = 12;

const PITCH_MAP: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

const NOTE2MIDI_PITCH_MAP: &[(char, i64)] = &[
    ('C', 0),
    ('D', 2),
    ('E', 4),
    ('F', 5),
    ('G', 7),
    ('A', 9),
    ('B', 11),
];

/// `midi2note` — Python's `round` rounds ties to even and `//` floors.
pub fn midi2note(midi: f64) -> String {
    let midi = midi.round_ties_even() as i64;
    let octave = midi.div_euclid(KEY_IN_OCTAVE) - 1;
    let pitch = PITCH_MAP[midi.rem_euclid(KEY_IN_OCTAVE) as usize];
    format!("{pitch}{octave}")
}

/// `note2midi` for a MIDI step letter and octave (the parser only ever builds
/// `"{step}{octave}"`, without accidentals).
pub fn note2midi(step: char, octave: i64, accidental: i64) -> Option<i64> {
    let offset = NOTE2MIDI_PITCH_MAP
        .iter()
        .find(|(letter, _)| *letter == step.to_ascii_uppercase())
        .map(|(_, offset)| *offset)?;
    Some(KEY_IN_OCTAVE * (octave + 1) + offset + accidental)
}

/// `float(Decimal(text))`.
pub fn parse_f64(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}
