//! `vsq` plugin — port of `libresvip.plugins.vsq`.
//!
//! Vocaloid 2 sequence files are Standard MIDI Files whose text events carry an
//! INI document (`DM:nnnn:` chunks). The parser re-assembles and reads that
//! document; the generator emits it again and rebuilds the MIDI container.

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_breath_importation, enable_gender_importation, enable_pitch_importation,
    enable_strength_importation, enable_volume_importation, EnumVariant, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::supported_charset_names;

use crate::plugin_infos;

pub mod constants;
pub mod generator;
pub mod ini;
pub mod midi;
pub mod parser;

pub struct VsqPlugin;

pub static PLUGIN: VsqPlugin = VsqPlugin;

static BREATH_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "IGNORE",
        value: "ignore",
        title: "Ignore all breath notes",
    },
    EnumVariant {
        name: "KEEP",
        value: "keep",
        title: "Keep as normal notes",
    },
];

impl SvsPlugin for VsqPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VSQ
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VSQ_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_strength_importation(),
            enable_gender_importation(),
            enable_breath_importation(),
            enable_volume_importation(),
            enable_pitch_importation(),
            OptionFieldDef::string("lyric_encoding", "Lyric text encoding", "SHIFT_JIS")
                .with_description(
                    "Unless the lyrics are garbled, this option should not be changed.",
                )
                .with_choices(supported_charset_names()),
            OptionFieldDef::enumerated(
                "breath",
                "The way to handle breath notes",
                "BreathOption",
                BREATH_VARIANTS,
                "IGNORE",
            ),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::int("ticks_per_beat", "Ticks per beat", 480).with_description(
                "Also known as parts per quarter, ticks per quarter note, the number of pulses per quarter note. This setting should not be changed unless you know what it is.",
            ),
            OptionFieldDef::string("lyric_encoding", "Lyric text encoding", "SHIFT_JIS")
                .with_description(
                    "Unless the lyrics are garbled, this option should not be changed.",
                )
                .with_choices(supported_charset_names()),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let breath = match options.str("breath", "ignore").trim() {
            "keep" => parser::BreathOption::Keep,
            "ignore" => parser::BreathOption::Ignore,
            other => {
                return Err(LibreSvipError::Validation(format!(
                    "invalid breath option: {other}"
                )))
            }
        };
        let input_options = parser::InputOptions {
            import_pitch: options.bool("import_pitch", true),
            import_volume: options.bool("import_volume", true),
            import_breath: options.bool("import_breath", true),
            import_gender: options.bool("import_gender", true),
            import_strength: options.bool("import_strength", true),
            lyric_encoding: options.str("lyric_encoding", "SHIFT_JIS"),
            breath,
        };
        parser::parse_project(&bytes, &input_options)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = generator::OutputOptions {
            ticks_per_beat: options.int("ticks_per_beat", 480),
            lyric_encoding: options.str("lyric_encoding", "SHIFT_JIS"),
        };
        let bytes = generator::generate_project(project, &output_options)?;
        std::fs::write(path, &bytes).at_path(path)?;
        Ok(())
    }
}
