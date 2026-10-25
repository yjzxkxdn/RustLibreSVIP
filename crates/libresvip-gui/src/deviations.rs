//! Documented differences from the Python reference, per format plugin.
//!
//! The "about this format" dialog prints these under "Modifications", so a user
//! can see at a glance whether a plugin is a plain port or intentionally
//! deviates. Keep in sync with `testdata/golden/<id>/meta.json` (the
//! `deviations` array) and the table in the README.

/// Deviation keys (English; translated by `App::t` when rendered).
pub fn for_plugin(id: &str) -> &'static [&'static str] {
    match id {
        "aisp" => &[
            "pitch points accumulate across items (upstream 2.9.2 self-extend bug, fixed)",
        ],
        "mid" | "svip3" => &[
            "the parser does not echo the lyric into the pronunciation field",
        ],
        "ustx" => &[
            "the parser does not echo the lyric into the pronunciation field",
            "reads USTX 0.10 masked pitch curves (pito / rpit) and note tuning; the reference accepts the type tags but drops the values",
        ],
        "svp" => &[
            "the phoneme layer is off by default (upstream writes normalize_phoneme for every note)",
        ],
        "ust" | "vpr" | "vsqx" | "y77" => &[
            "MIDI key 0 (C-1) is a real pitch (upstream treated 0 as \"no pitch\")",
        ],
        "vog" => &["ZIP timestamps come from the wall clock, so two dumps differ"],
        "vspx" => &[
            "no-pitch breaks are inserted only at real gaps (upstream compared an unshifted time with a shifted tick)",
            "the parser does not echo the lyric into the pronunciation field",
        ],
        _ => &[],
    }
}
