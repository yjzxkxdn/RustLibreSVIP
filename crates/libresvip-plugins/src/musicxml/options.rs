//! `libresvip/plugins/musicxml/options.py`.

use libresvip_core::options::{OptionFieldDef, Options};

#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_tempo: bool,
    pub import_dynamics: bool,
    pub apply_fermata_stretch: bool,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_tempo: true,
            import_dynamics: true,
            apply_fermata_stretch: true,
        }
    }
}

impl InputOptions {
    pub fn from_options(options: &Options) -> Self {
        Self {
            import_tempo: options.bool("import_tempo", true),
            import_dynamics: options.bool("import_dynamics", true),
            apply_fermata_stretch: options.bool("apply_fermata_stretch", true),
        }
    }
}

pub fn input_options() -> Vec<OptionFieldDef> {
    vec![
        OptionFieldDef::bool("import_tempo", "Import tempo changes", true),
        OptionFieldDef::bool(
            "import_dynamics",
            "Import dynamics as volume curve",
            true,
        ),
        OptionFieldDef::bool(
            "apply_fermata_stretch",
            "Extend fermata-bearing notes (matches MuseScore playback)",
            true,
        ),
    ]
}
