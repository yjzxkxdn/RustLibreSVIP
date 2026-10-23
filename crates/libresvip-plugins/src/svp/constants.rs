//! Port of `libresvip.plugins.svp.constants`.

/// Ticks per "position" unit of the SynthV file format.
pub const TICK_RATE: i64 = 1_470_000;

pub const DEFAULT_PITCH_TRANSITION: f64 = 0.0;
pub const DEFAULT_PITCH_PORTAMENTO: f64 = 0.07;
pub const DEFAULT_PITCH_DEPTH: f64 = 0.15;
pub const DEFAULT_VIBRATO_START: f64 = 0.25;
pub const DEFAULT_VIBRATO_FADE: f64 = 0.2;
pub const DEFAULT_VIBRATO_DEPTH: f64 = 1.0;
pub const DEFAULT_VIBRATO_FREQUENCY: f64 = 5.5;
pub const DEFAULT_VIBRATO_PHASE: f64 = 0.0;
pub const DEFAULT_VIBRATO_JITTER: f64 = 1.0;
pub const SYSTEM_PITCH_PORTAMENTO: f64 = 0.1;
pub const SYSTEM_PITCH_DEPTH: f64 = 0.0;
pub const DEFAULT_PHONE_RATIO: f64 = 1.8;

#[derive(Clone, Copy, Debug)]
pub struct DefaultDurations {
    pub stop: f64,
    pub affricate: f64,
    pub fricative: f64,
    pub aspirate: f64,
    pub liquid: f64,
    pub nasal: f64,
    pub vowel: f64,
    pub semivowel: f64,
    pub diphthong: f64,
}

pub const DEFAULT_DURATIONS: DefaultDurations = DefaultDurations {
    stop: 0.10,
    affricate: 0.125,
    fricative: 0.125,
    aspirate: 0.094,
    liquid: 0.062,
    nasal: 0.094,
    vowel: 0.0,
    semivowel: 0.055,
    diphthong: 0.0,
};

impl DefaultDurations {
    pub fn get(&self, category: &str) -> Option<f64> {
        match category {
            "stop" => Some(self.stop),
            "affricate" => Some(self.affricate),
            "fricative" => Some(self.fricative),
            "aspirate" => Some(self.aspirate),
            "liquid" => Some(self.liquid),
            "nasal" => Some(self.nasal),
            "vowel" => Some(self.vowel),
            "semivowel" => Some(self.semivowel),
            "diphthong" => Some(self.diphthong),
            _ => None,
        }
    }
}
