//! Port of `libresvip.plugins.svip3.constants`.

/// Singer used when a track's AI singer name is unknown.
pub const DEFAULT_SINGER_ID: &str = "7d0c0cfc-00b3-4dca-b7b0-d20b634b531a";
/// `MIN_NOTE_DURATION` — notes shorter than this get `TOO_SHORT`.
pub const MIN_NOTE_DURATION: f64 = 0.06;
/// `MAX_NOTE_DURATION` — notes longer than this get `TOO_LONG`.
pub const MAX_NOTE_DURATION: f64 = 6.0;

/// `Svip3TrackType.SINGING_TRACK` as the URL produced by `urljoin`.
pub const SINGING_TRACK_URL: &str = "type.googleapis.com/xstudio.proto.SingingTrack";
/// `Svip3TrackType.AUDIO_TRACK` as the URL produced by `urljoin`.
pub const AUDIO_TRACK_URL: &str = "type.googleapis.com/xstudio.proto.AudioTrack";

/// `Svip3NoteLengthValidateTag`.
pub mod length_validate_tag {
    pub const NONE: i32 = 0;
    pub const TOO_LONG: i32 = 1;
    pub const TOO_SHORT: i32 = 2;
}

/// `libresvip.plugins.svip3.color_pool._colors`.
pub const COLORS: [&str; 10] = [
    "#24936E", "#B54434", "#B5495B", "#A8497A", "#4F726C", "#939650", "#CA7A2C", "#DB4D6D",
    "#77428D", "#005CAF",
];
