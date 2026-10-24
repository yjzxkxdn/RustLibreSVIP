//! DiffScope Project Exchange (`.dspx`) plugin — port of
//! `libresvip.plugins.dspx`.

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{enable_pitch_importation, EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod generator;
pub mod model;
pub mod param_utils;
pub mod parser;
pub mod pyutil;
pub mod vibrato_utils;

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

const PITCH_IMPORT_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "EDITED_AND_ORIGINAL",
        value: "edited-and-original",
        title: "Edited and original pitch",
    },
    EnumVariant {
        name: "EDITED_ONLY",
        value: "edited-only",
        title: "Edited pitch only",
    },
];

const VIBRATO_IMPORT_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "NONE",
        value: "none",
        title: "Do not import vibrato",
    },
    EnumVariant {
        name: "PRESERVE",
        value: "preserve",
        title: "Preserve vibrato information on notes",
    },
    EnumVariant {
        name: "BAKE_TO_PITCH",
        value: "bake-to-pitch",
        title: "Bake vibrato into the pitch curve",
    },
];

pub struct DspxPlugin;

pub static PLUGIN: DspxPlugin = DspxPlugin;

impl SvsPlugin for DspxPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_DSPX
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_DSPX_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_pitch_importation(),
            OptionFieldDef::enumerated(
                "pitch_import_mode",
                "Pitch import mode",
                "PitchImportMode",
                PITCH_IMPORT_VARIANTS,
                "EDITED_AND_ORIGINAL",
            ),
            OptionFieldDef::enumerated(
                "vibrato_import_mode",
                "Vibrato import mode",
                "VibratoImportMode",
                VIBRATO_IMPORT_VARIANTS,
                "PRESERVE",
            ),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::bool("export_pitch", "Export pitch curve", true),
            OptionFieldDef::bool("preserve_vibrato", "Preserve vibrato", true),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let raw = if bytes.starts_with(&ZSTD_MAGIC) {
            libresvip_core::compression::decompress(&bytes[..])?
        } else {
            bytes
        };
        let text = String::from_utf8(raw)
            .map_err(|error| LibreSvipError::Validation(format!("invalid UTF-8: {error}")))?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        if !value.is_object() {
            return Err(LibreSvipError::InvalidFileType);
        }
        let version = value.get("version").and_then(|version| version.as_str());
        match version {
            Some("1.0.0") => {}
            other => {
                return Err(LibreSvipError::UnsupportedProjectVersion(format!(
                    "{other:?}"
                )))
            }
        }
        let model: model::DspxModel = serde_json::from_value(value)?;
        Ok(parser::DspxParser::new(options, path).parse_project(&model))
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let model = generator::generate_project(project, options);
        let data = generator::dump_model(&model)?;
        libresvip_core::io::write(path, data).at_path(path)?;
        Ok(())
    }
}
