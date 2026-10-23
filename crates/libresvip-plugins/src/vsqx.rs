//! `vsqx` plugin — port of `libresvip.plugins.vsqx`.
//!
//! Reads and writes Vocaloid 3/4 sequence XML files (`*.vsqx`), including the
//! pitch-bend/parameter controllers and the legacy `vsq3` element names.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_breath_importation, enable_gender_importation, enable_instrumental_track_importation,
    enable_pitch_importation, enable_strength_importation, enable_volume_importation, EnumVariant,
    OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod generator;
pub mod parser;
pub mod phonemes;
pub mod vocaloid;
pub mod xml;

/// `constants.BPM_RATE` — VSQX stores BPM × 100.
pub const BPM_RATE: f64 = 100.0;

pub struct VsqxPlugin;

pub static PLUGIN: VsqxPlugin = VsqxPlugin;

static VSQX_VERSION_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "VSQ3",
        value: "3",
        title: "VSQx 3",
    },
    EnumVariant {
        name: "VSQ4",
        value: "4",
        title: "VSQx 4",
    },
];

static VOCALOID_LANGUAGE_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "JAPANESE",
        value: "0",
        title: "日本語",
    },
    EnumVariant {
        name: "ENGLISH",
        value: "1",
        title: "English",
    },
    EnumVariant {
        name: "KOREAN",
        value: "2",
        title: "한국어",
    },
    EnumVariant {
        name: "SPANISH",
        value: "3",
        title: "Español",
    },
    EnumVariant {
        name: "SIMPLIFIED_CHINESE",
        value: "4",
        title: "简体中文",
    },
];

impl SvsPlugin for VsqxPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VSQX
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VSQX_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        // pydantic collects the mixin fields in reverse MRO order.
        vec![
            enable_strength_importation(),
            enable_gender_importation(),
            enable_breath_importation(),
            enable_volume_importation(),
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
            OptionFieldDef::bool("combine_syllables", "Combine syllables", false)
                .with_description("Combine multisyllabic lyrics into single words"),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        let mut version_field = OptionFieldDef::enumerated(
            "vsqx_version",
            "VSQX Version",
            "VsqxVersion",
            VSQX_VERSION_VARIANTS,
            "VSQ4",
        );
        // `str(VocaloidLanguage.SIMPLIFIED_CHINESE)` prints the integer value.
        version_field.default_repr = None;
        let mut language_field = OptionFieldDef::enumerated(
            "default_lang_id",
            "Default language",
            "VocaloidLanguage",
            VOCALOID_LANGUAGE_VARIANTS,
            "SIMPLIFIED_CHINESE",
        )
        .with_description("Default language id of voicebank and notes");
        language_field.default_repr = None;
        vec![
            version_field,
            OptionFieldDef::bool("pretty_xml", "Pretty XML", true)
                .with_description("Whether to output pretty XML"),
            language_field,
            OptionFieldDef::string("default_comp_id", "Default Comp ID", "BETDB8W6KWZPYEB9")
                .with_description("Default comp_id of voicebank"),
            OptionFieldDef::string("default_singer_name", "Default Singer Name", "Tianyi_CHN")
                .with_description("Default singer name of voicebank"),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let input_options = parser::InputOptions {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
            import_volume: options.bool("import_volume", true),
            import_breath: options.bool("import_breath", true),
            import_gender: options.bool("import_gender", true),
            import_strength: options.bool("import_strength", true),
            combine_syllables: options.bool("combine_syllables", false),
        };
        parser::parse_project(&bytes, &input_options, path)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let version = match options.str("vsqx_version", "4").trim() {
            "3" => parser::VsqxVersion::Vsq3,
            "4" => parser::VsqxVersion::Vsq4,
            other => {
                return Err(libresvip_core::error::LibreSvipError::Validation(format!(
                    "invalid vsqx_version: {other}"
                )))
            }
        };
        let output_options = generator::OutputOptions {
            vsqx_version: version,
            pretty_xml: options.bool("pretty_xml", true),
            default_lang_id: options.int("default_lang_id", 4),
            default_comp_id: options.str("default_comp_id", "BETDB8W6KWZPYEB9"),
            default_singer_name: options.str("default_singer_name", "Tianyi_CHN"),
        };
        let text = generator::generate_project(project, &output_options)?;
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
