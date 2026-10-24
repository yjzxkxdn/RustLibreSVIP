//! Read-only format capability metadata.
//!
//! Inspired by UtaFormatix3's `Format` enum metadata (`possibleLyricsTypes`,
//! `availableFeaturesForGeneration`): the UI and the generated docs can ask
//! "what does this format carry?" without poking at plugin internals.
//!
//! The data here is *descriptive, never prescriptive*: conversion logic must
//! not branch on it. A feature is declared when the format carries the
//! information in at least one direction (import into the hub model or export
//! from it); which directions exist is already answered by
//! [`crate::plugin::SvsPlugin::readable`] / `writable`.

/// A piece of project information a format can carry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FormatFeature {
    /// The pitch parameter curve.
    Pitch,
    /// The volume/dynamics parameter curve (not the per-track gain knob).
    Volume,
    /// The breath parameter curve.
    Breath,
    /// The gender/character parameter curve.
    Gender,
    /// The strength/tension parameter curve.
    Strength,
    /// A per-note pronunciation layer on top of the lyric.
    Phonemes,
    /// Note-level vibrato parameters (not vibrato pre-baked into pitch).
    Vibrato,
    /// Instrumental/audio reference tracks alongside singing tracks.
    InstrumentalTracks,
}

impl FormatFeature {
    /// Every feature, in declaration order.
    pub const ALL: [FormatFeature; 8] = [
        FormatFeature::Pitch,
        FormatFeature::Volume,
        FormatFeature::Breath,
        FormatFeature::Gender,
        FormatFeature::Strength,
        FormatFeature::Phonemes,
        FormatFeature::Vibrato,
        FormatFeature::InstrumentalTracks,
    ];

    /// Stable machine key, also used by the CLI `--json` dump.
    pub const fn key(self) -> &'static str {
        match self {
            FormatFeature::Pitch => "pitch",
            FormatFeature::Volume => "volume",
            FormatFeature::Breath => "breath",
            FormatFeature::Gender => "gender",
            FormatFeature::Strength => "strength",
            FormatFeature::Phonemes => "phonemes",
            FormatFeature::Vibrato => "vibrato",
            FormatFeature::InstrumentalTracks => "instrumental_tracks",
        }
    }

    /// One-line human description.
    pub const fn description(self) -> &'static str {
        match self {
            FormatFeature::Pitch => "pitch parameter curve",
            FormatFeature::Volume => "volume parameter curve",
            FormatFeature::Breath => "breath parameter curve",
            FormatFeature::Gender => "gender parameter curve",
            FormatFeature::Strength => "strength parameter curve",
            FormatFeature::Phonemes => "per-note pronunciation layer",
            FormatFeature::Vibrato => "note-level vibrato parameters",
            FormatFeature::InstrumentalTracks => "instrumental (audio) tracks",
        }
    }
}

/// What a format can express. Declared per plugin identifier; see
/// `libresvip_plugins::capabilities`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FormatCapabilities {
    /// Information this format carries (import or export side).
    pub features: &'static [FormatFeature],
    /// At most this many tracks fit into one output file. `None` means the
    /// format takes as many tracks as the project has.
    pub max_tracks_per_file: Option<usize>,
}

impl FormatCapabilities {
    /// A format with none of the listed features and no track limit.
    pub const EMPTY: FormatCapabilities = FormatCapabilities {
        features: &[],
        max_tracks_per_file: None,
    };

    /// Whether `feature` is declared.
    pub fn has(&self, feature: FormatFeature) -> bool {
        self.features.contains(&feature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_stable() {
        let mut keys: Vec<&str> = FormatFeature::ALL.iter().map(|f| f.key()).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count, "feature keys must be unique");
        assert_eq!(FormatFeature::Pitch.key(), "pitch");
    }

    #[test]
    fn empty_capabilities_have_no_features() {
        let caps = FormatCapabilities::EMPTY;
        assert!(!caps.has(FormatFeature::Pitch));
        assert_eq!(caps.max_tracks_per_file, None);
    }
}
