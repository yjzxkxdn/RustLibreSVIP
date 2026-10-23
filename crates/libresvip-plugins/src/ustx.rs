//! USTX (OpenUtau project file) plugin — port of `libresvip.plugins.ustx`.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, EnumVariant, OptionFieldDef,
    Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::to_unicode;

use crate::plugin_infos;

mod base_pitch;
mod generator;
mod lyric_util;
mod model;
mod parser;
mod time_axis;

/// `PlusHandlingMode`.
static PLUS_HANDLING_MODE_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "AUTO",
        value: "auto",
        title: "Auto determine by phonemizer language",
    },
    EnumVariant {
        name: "MONOSYLLABIC",
        value: "monosyllabic",
        title: "Monosyllabic languages: treating +, +~ and +* as slur notes",
    },
    EnumVariant {
        name: "POLYSYLLABIC",
        value: "polysyllabic",
        title: "Polysyllabic languages: treating + as slur notes, and treating +~ or +* as syllable placeholders",
    },
];

/// `OpenUtauEnglishPhonemizerCompatibility`.
static ENGLISH_PHONEMIZER_COMPATIBILITY_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "NON_ARPA",
        value: "non-arpa",
        title: "Incompatible with ARPAsing-series Phonemizers",
    },
    EnumVariant {
        name: "ARPA",
        value: "arpa",
        title: "Compatible with ARPAsing-series Phonemizers",
    },
];

pub struct UstxPlugin;

pub static PLUGIN: UstxPlugin = UstxPlugin;

impl SvsPlugin for UstxPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_USTX
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_USTX_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
            OptionFieldDef::enumerated(
                "plus_handling_mode",
                "Plus sign handling mode",
                "PlusHandlingMode",
                PLUS_HANDLING_MODE_VARIANTS,
                "AUTO",
            )
            .with_description("How to handle the + symbol in lyrics when importing USTX"),
            OptionFieldDef::string("breath_lyrics", "Breath lyrics", "Asp AP").with_description(
                "Special lyrics that will be recognized as breath notes only when immediately \
                 followed by a regular note, each separated by a space",
            ),
            OptionFieldDef::string("silence_lyrics", "Silence lyrics", "R SP").with_description(
                "Special lyrics that will be ignored, each separated by a space",
            ),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::enumerated(
            "english_phonemizer_compatibility",
            "The way to handle english multisyllabic words",
            "OpenUtauEnglishPhonemizerCompatibility",
            ENGLISH_PHONEMIZER_COMPATIBILITY_VARIANTS,
            "NON_ARPA",
        )
        .with_description("Compatibility with ARPAsing-series Phonemizer")]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let input_options = parser::InputOptions {
            import_pitch: options.bool("import_pitch", true),
            import_instrumental_track: options.bool("import_instrumental_track", true),
            plus_handling_mode: parser::PlusHandlingMode::parse(
                &options.str("plus_handling_mode", "auto"),
            ),
            breath_lyrics: options.str("breath_lyrics", "Asp AP"),
            silence_lyrics: options.str("silence_lyrics", "R SP"),
        };
        let bytes = std::fs::read(path).at_path(path)?;
        let text = to_unicode(&bytes);
        let mut ustx_project: model::UstxProject = serde_yaml::from_str(&text)?;
        parser::resolve_audio_paths(&mut ustx_project, path);
        parser::parse_project(&ustx_project, &input_options)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = generator::OutputOptions {
            arpa: options.str("english_phonemizer_compatibility", "non-arpa") == "arpa",
        };
        let ustx_project = generator::UstxGenerator::new(&output_options)
            .generate_project(project)?;
        let text = serde_yaml::to_string(&ustx_project)?;
        std::fs::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}

