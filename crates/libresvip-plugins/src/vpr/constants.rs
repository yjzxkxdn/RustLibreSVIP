//! Port of `libresvip.plugins.vpr.constants`.

/// Default phoneme for Chinese lyrics.
pub const DEFAULT_CHINESE_PHONEME: &str = "l a";
/// Default phoneme for Japanese lyrics.
pub const DEFAULT_JAPANESE_PHONEME: &str = "4 a";
/// Controller name of the pitch-bend curve.
#[allow(dead_code)]
pub const PITCH_BEND_NAME: &str = "pitchBend";
/// Controller name of the pitch-bend sensitivity curve.
#[allow(dead_code)]
pub const PITCH_BEND_SENSITIVITY_NAME: &str = "pitchBendSens";
/// VPR stores BPM × 100.
pub const BPM_RATE: f64 = 100.0;
