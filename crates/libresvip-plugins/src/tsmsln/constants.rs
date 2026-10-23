//! Port of `libresvip.plugins.tsmsln.constants`.

/// Default phoneme used when a lyric is neither kana nor romaji.
pub const DEFAULT_PHONEME: &str = "r,a";
/// Ticks per time unit, per BPM.
pub const TIME_UNIT_AS_TICKS_PER_BPM: f64 = 4.8 / 120.0;
/// Octave offset of the VoiSona pitch representation.
pub const OCTAVE_OFFSET: i64 = -1;
/// Ticks per quarter note in VoiSona files.
pub const TICK_RATE: f64 = 2.0;
/// Minimum length added to a generated parameter curve.
pub const MIN_DATA_LENGTH: i64 = 500;
/// Value used to mark a gap in the parameter curve.
pub const TEMP_VALUE_AS_NULL: f64 = -1.0;
