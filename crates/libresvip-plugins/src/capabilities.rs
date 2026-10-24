//! Per-format capability declarations (see
//! `libresvip_core::capabilities` for the types).
//!
//! The table states, per plugin identifier, which project information the
//! format carries in at least one direction (import into the hub model or
//! export from it) and whether one output file can hold only a single track.
//!
//! Entries were derived from what each parser/generator actually touches
//! (hub `Params` curves, `pronunciation`/`edited_phones`, hub `VibratoParam`,
//! `Track::Instrumental`, single-track exporters). When a format gains or
//! loses support for something, update its row here in the same change.

use libresvip_core::capabilities::{FormatCapabilities, FormatFeature};

use FormatFeature::{
    Breath, Gender, InstrumentalTracks as Instrumental, Phonemes, Pitch, Strength, Vibrato,
    Volume,
};

macro_rules! caps {
    (single $($feature:ident),* $(,)?) => {
        FormatCapabilities {
            features: &[$($feature),*],
            max_tracks_per_file: Some(1),
        }
    };
    ($($feature:ident),* $(,)?) => {
        FormatCapabilities {
            features: &[$($feature),*],
            max_tracks_per_file: None,
        }
    };
}

/// (plugin identifier, capabilities), sorted by identifier.
pub const ALL_CAPABILITIES: &[(&str, FormatCapabilities)] = &[
    ("ace", caps![Pitch, Strength, Phonemes, Instrumental]),
    (
        "acep",
        caps![
            Pitch,
            Volume,
            Breath,
            Gender,
            Strength,
            Phonemes,
            Vibrato,
            Instrumental
        ],
    ),
    ("aisp", caps![Pitch, Phonemes, Instrumental]),
    ("ass", caps![single]),
    ("ccs", caps![Pitch, Phonemes, Instrumental]),
    ("ds", caps![single Pitch, Gender, Phonemes]),
    ("dspx", caps![Pitch, Phonemes, Vibrato, Instrumental]),
    ("dv", caps![Pitch, Phonemes, Vibrato, Instrumental]),
    (
        "gj",
        caps![Pitch, Volume, Phonemes, Instrumental],
    ),
    (
        "json",
        caps![
            Pitch,
            Volume,
            Breath,
            Gender,
            Strength,
            Phonemes,
            Vibrato,
            Instrumental
        ],
    ),
    ("lrc", caps![single]),
    ("mid", caps![Pitch, Phonemes]),
    (
        "msq",
        caps![Pitch, Volume, Breath, Gender, Strength, Phonemes, Instrumental],
    ),
    ("mtp", caps![Pitch, Volume, Phonemes, Instrumental]),
    ("musicxml", caps![Volume]),
    ("nn", caps![single Pitch, Vibrato]),
    ("ppsf", caps![Pitch, Phonemes, Instrumental]),
    ("ps_project", caps![Pitch, Phonemes, Instrumental]),
    ("s5p", caps![Pitch, Phonemes, Instrumental]),
    ("srt", caps![single]),
    ("svg", caps![single Pitch]),
    (
        "svip",
        caps![
            Pitch,
            Volume,
            Breath,
            Gender,
            Strength,
            Phonemes,
            Vibrato,
            Instrumental
        ],
    ),
    ("svip3", caps![Pitch, Phonemes, Vibrato, Instrumental]),
    (
        "svp",
        caps![
            Pitch,
            Volume,
            Breath,
            Gender,
            Strength,
            Phonemes,
            Vibrato,
            Instrumental
        ],
    ),
    ("tlp", caps![Pitch, Phonemes, Instrumental]),
    ("tlpx", caps![Pitch, Phonemes, Instrumental]),
    ("tsmsln", caps![single Pitch, Vibrato]),
    ("tssln", caps![Pitch, Phonemes, Instrumental]),
    ("ufdata", caps![Pitch]),
    ("ust", caps![single Pitch]),
    ("ustx", caps![Pitch, Phonemes, Vibrato, Instrumental]),
    ("vfp", caps![Pitch, Phonemes, Vibrato, Instrumental]),
    ("vog", caps![Phonemes]),
    (
        "vpr",
        caps![Pitch, Volume, Breath, Gender, Strength, Instrumental],
    ),
    ("vshp", caps![Pitch, Volume, Breath, Gender, Instrumental]),
    ("vspx", caps![Pitch, Phonemes, Instrumental]),
    ("vsq", caps![Pitch, Volume, Breath, Gender, Strength, Phonemes]),
    (
        "vsqx",
        caps![Pitch, Volume, Breath, Gender, Strength, Phonemes, Instrumental],
    ),
    ("vsp", caps![Pitch, Instrumental]),
    ("vvproj", caps![Pitch]),
    ("vxf", caps![Pitch, Phonemes]),
    (
        "xvsq",
        caps![Pitch, Volume, Breath, Gender, Strength, Phonemes, Instrumental],
    ),
    ("y77", caps![single Pitch, Phonemes]),
];

/// Capabilities of a registered plugin, by identifier (or any of its
/// suffixes, matching `plugin_by_value`).
pub fn capabilities(value: &str) -> Option<FormatCapabilities> {
    let id = crate::plugin_by_value(value).map(|plugin| plugin.info().id)?; // normalize suffixes
    ALL_CAPABILITIES
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, caps)| *caps)
}

/// Capabilities for identifiers without going through the plugin registry.
pub fn capabilities_by_id(id: &str) -> Option<FormatCapabilities> {
    ALL_CAPABILITIES
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, caps)| *caps)
}

/// Subtitle/parameter exporters whose output holds exactly one track.
pub fn single_track_formats() -> Vec<&'static str> {
    ALL_CAPABILITIES
        .iter()
        .filter(|(_, caps)| caps.max_tracks_per_file == Some(1))
        .map(|(id, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registered plugin must have exactly one declaration, and every
    /// declaration must correspond to a registered plugin.
    #[test]
    fn table_covers_every_registered_plugin() {
        let registered: Vec<&str> = crate::all_plugins()
            .iter()
            .map(|plugin| plugin.info().id)
            .collect();
        let declared: Vec<&str> = ALL_CAPABILITIES.iter().map(|(id, _)| *id).collect();
        let mut sorted_declared = declared.clone();
        sorted_declared.sort_unstable();
        assert_eq!(
            sorted_declared, registered,
            "capability table and plugin registry disagree"
        );
    }

    #[test]
    fn lookup_accepts_identifier_and_suffix() {
        assert!(capabilities("svp").is_some());
        assert!(capabilities(".svp").is_none(), "bare suffix only");
        let by_id = capabilities_by_id("svp").expect("svp declared");
        assert!(by_id.has(Pitch) && by_id.has(Vibrato));
        assert!(!by_id.has(Vibrato) || by_id.features.contains(&Vibrato));
        assert_eq!(capabilities_by_id("nope"), None);
    }

    #[test]
    fn single_track_exporters_are_declared() {
        let singles = single_track_formats();
        for expected in ["ass", "ds", "lrc", "nn", "srt", "svg", "tsmsln", "ust", "y77"] {
            assert!(singles.contains(&expected), "{expected} must be single-track");
        }
    }
}
