//! Mikoto Studio project (`.msq`) plugin — port of the upstream
//! `experimental/msq` format. Rust-port addition of an upstream plugin the
//! Python program itself does not load (it lives outside `libresvip.plugins`).

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod generator;
pub mod model;
pub mod parser;

pub struct MsqPlugin;

pub static PLUGIN: MsqPlugin = MsqPlugin;

impl SvsPlugin for MsqPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_MSQ
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_MSQ_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::bool("import_pitch", "Import pitch curve", true),
            OptionFieldDef::bool("import_volume", "Import volume curve", true),
            OptionFieldDef::bool("import_breath", "Import breath curve", true),
            OptionFieldDef::bool("import_gender", "Import gender curve", true),
            OptionFieldDef::bool("import_strength", "Import strength curve", true),
            OptionFieldDef::bool(
                "import_instrumental_track",
                "Import instrumental tracks",
                true,
            ),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let text = libresvip_core::io::read_to_string(path).at_path(path)?;
        let msq_project: model::MikotoStudioSequenceFormat = serde_json::from_str(&text)?;
        let input_options = parser::InputOptions {
            import_pitch: options.bool("import_pitch", true),
            import_volume: options.bool("import_volume", true),
            import_breath: options.bool("import_breath", true),
            import_gender: options.bool("import_gender", true),
            import_strength: options.bool("import_strength", true),
            import_instrumental_track: options.bool("import_instrumental_track", true),
        };
        parser::MsqParser::new(input_options).parse_project(&msq_project)
    }

    fn dump(&self, path: &Path, project: &Project, _options: &Options) -> Result<()> {
        let mut generator = generator::MsqGenerator::new();
        let msq_project = generator.generate_project(project);
        // Python: `model_dump_json(by_alias=True, indent=2)`.
        let text = serde_json::to_string_pretty(&msq_project)?;
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
