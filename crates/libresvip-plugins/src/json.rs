//! OpenSVIP JSON plugin — port of `libresvip.plugins.json`.
//!
//! `load` deserialises the internal [`Project`] model directly; `dump` writes it
//! back with pydantic-compatible field order and aliases.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

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
        Ok(serde_json::from_str(&text)?)
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
