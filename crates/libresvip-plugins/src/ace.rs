//! ACE Virtual Singer raw JSON (`.ace`) plugin — port of
//! `libresvip.plugins.ace`.

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, enable_strength_importation,
    EnumVariant, OptionFieldDef, OptionKind, OptionValue, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod generator;
pub mod model;
pub mod parser;
pub mod pyutil;

pub struct AcePlugin;

pub static PLUGIN: AcePlugin = AcePlugin;

const BPM_SOURCE_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "AUTOMATIC",
        value: "automatic",
        title: "Automatic",
    },
    EnumVariant {
        name: "SONG_INFO",
        value: "song_info",
        title: "Song information",
    },
    EnumVariant {
        name: "BGM_INFO",
        value: "bgm_info",
        title: "Accompaniment information",
    },
];

impl SvsPlugin for AcePlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_ACE
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_ACE_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_strength_importation(),
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
            // `BpmSource` is a `StrEnum`: Python prints the type name and the
            // member value as default, and (unlike `_value_`-annotated enums)
            // no "Available values" block. The CLI prompt still offers the
            // three member values, hence the non-empty variant list.
            OptionFieldDef {
                key: "bpm_source",
                title: "BPM source",
                description: Some(
                    "Use the accompaniment BPM when it is present, or fall back to the song BPM.",
                ),
                kind: OptionKind::Enum {
                    type_name: "BpmSource",
                    variants: BPM_SOURCE_VARIANTS,
                },
                default: OptionValue::Str("automatic".to_owned()),
                default_repr: Some("automatic".to_owned()),
                choices: None,
                // A plain `StrEnum` has no `_value_` model, so `plugin detail`
                // prints the field but no "Available values:" block.
                show_enum_values: false,
            },
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::string("author", "Author", ""),
            OptionFieldDef::bool("export_pitch", "Export pitch curve", true),
            OptionFieldDef::bool("indented", "Indent JSON", true),
            OptionFieldDef::string("key", "Musical key", "C"),
            OptionFieldDef::int("role_id", "Default singer ID", 0),
            OptionFieldDef::string("role_name", "Default singer name", ""),
            OptionFieldDef::string("song_name", "Song name", "New Project"),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(error) if error.to_string().contains("trailing characters") => {
                let mut stream = serde_json::Deserializer::from_slice(&bytes).into_iter();
                match stream.next() {
                    Some(Ok(value)) => value,
                    Some(Err(stream_error)) => return Err(LibreSvipError::Json(stream_error)),
                    None => return Err(LibreSvipError::Json(error)),
                }
            }
            Err(error) => return Err(LibreSvipError::Json(error)),
        };
        let mut ace_project: model::AceProject = serde_json::from_value(value)?;
        ace_project.normalize();
        parser::AceParser::new(options, path).parse_project(&ace_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let ace_project = generator::generate_project(project, options);
        let text = if options.bool("indented", true) {
            generator::dump_indented(&ace_project)?
        } else {
            generator::dump_compact(&ace_project)?
        };
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
