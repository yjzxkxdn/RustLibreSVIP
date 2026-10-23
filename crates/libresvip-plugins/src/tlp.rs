//! TuneLab legacy (`.tlp`) plugin — port of `libresvip.plugins.tlp`.
//!
//! `load` reads a UTF-8 JSON document and `dump` writes `json.dumps` output with
//! the default separators and `ensure_ascii=False`, so the writer lives in
//! [`pyjson`] rather than going through serde_json.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::to_unicode;

use crate::plugin_infos;

pub mod generator;
pub mod model;
pub mod music;
pub mod parser;
pub mod pyjson;

pub struct TuneLabPlugin;

pub static PLUGIN: TuneLabPlugin = TuneLabPlugin;

impl SvsPlugin for TuneLabPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_TLP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_TLP_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        // pydantic collects the mixin fields in reverse MRO order.
        vec![
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let text = to_unicode(&bytes);
        let value = pyjson::parse(&text)?;
        let tlp_project = model::TuneLabProject::from_json(&value)?;
        let input_options = parser::InputOptions {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
        };
        parser::TuneLabParser::new(input_options).parse_project(&tlp_project)
    }

    fn dump(&self, path: &Path, project: &Project, _options: &Options) -> Result<()> {
        let tlp_project = generator::TuneLabGenerator::generate_project(project)?;
        let text = pyjson::dumps(&tlp_project.to_json(), false, false);
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
