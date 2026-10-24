//! OpenSVIP JSON plugin — port of `libresvip.plugins.json`.
//!
//! `load` deserialises the internal [`Project`] model directly; `dump` writes it
//! back with pydantic-compatible field order and aliases.

use std::path::Path;

use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

/// The newest OpenSVIP `Version` major this build was written against.
const SUPPORTED_OPENSVIP_MAJOR: u32 = 1;

pub struct JsonPlugin;

pub static PLUGIN: JsonPlugin = JsonPlugin;

impl SvsPlugin for JsonPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_JSON
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_JSON_VERSION
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::bool(
            "indented",
            "Generate JSON file with indentation",
            false,
        )
        .with_description(
            "The indented format is easier to read and modify, but it will take up a larger file size.",
        )]
    }

    fn load(&self, path: &Path, _options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let text = String::from_utf8_lossy(&bytes);
        let project: Project = serde_json::from_str(&text)?;
        // OpenSVIP is a third-party public specification; a newer major
        // version may carry fields this build silently drops.
        if let Some(major) = project
            .version
            .split('.')
            .next()
            .and_then(|part| part.parse::<u32>().ok())
        {
            if major > SUPPORTED_OPENSVIP_MAJOR {
                show_warning_kind(
                    WarningKind::IncompatibleVersion,
                    format!(
                        "OpenSVIP version {} is newer than the supported major version {}; fields may be dropped",
                        project.version, SUPPORTED_OPENSVIP_MAJOR
                    ),
                );
            }
        }
        Ok(project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let indented = options.bool("indented", false);
        let text = if indented {
            serde_json::to_string_pretty(project)?
        } else {
            serde_json::to_string(project)?
        };
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
