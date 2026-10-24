//! Gejigeji project (`.gj`) plugin — port of the upstream `deprecated/gj`
//! format (Gejigeji v2.0 JSON spec). Rust-port addition of an upstream plugin
//! that the Python program itself no longer loads (it lives outside
//! `libresvip.plugins`); see the README known-differences section.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod constants;
pub mod generator;
pub mod model;
pub mod parser;

pub struct GjPlugin;

pub static PLUGIN: GjPlugin = GjPlugin;

impl SvsPlugin for GjPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_GJ
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_GJ_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::bool(
                "import_instrumental_track",
                "Import instrumental tracks",
                true,
            ),
            OptionFieldDef::bool("import_pitch", "Import pitch curve", true),
            OptionFieldDef::bool("import_volume", "Import volume curve", true),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::int(
                "down_sample",
                "Average sampling interval for the volume parameter",
                32,
            )
            .with_description(
                "The unit is Tick. The larger the value, the smoother the editor; the smaller the value, the more accurate the volume parameter.",
            ),
            OptionFieldDef::string("singer", "Default singer", "扇宝"),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        // `utf-8-sig`: strip a BOM like Python's codec does.
        let text = String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes));
        let gjgj_project: model::GjgjProject = serde_json::from_str(&text)?;
        let input_options = parser::InputOptions {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
            import_volume: options.bool("import_volume", true),
        };
        parser::GjgjParser::new(input_options)
            .parse_project(&gjgj_project)
            .map_err(libresvip_core::LibreSvipError::msg)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = generator::OutputOptions {
            down_sample: options.int("down_sample", 32),
            singer: options.str("singer", "扇宝"),
        };
        let mut generator = generator::GjgjGenerator::new(output_options);
        let gjgj_project = generator.generate_project(project);
        // Python: `json.dumps(..., ensure_ascii=False, separators=(",", ":"))`
        // with `exclude_none=True`, written as utf-8-sig.
        let mut data = serde_json::to_vec(&gjgj_project)?;
        let mut out = vec![0xEF, 0xBB, 0xBF];
        out.append(&mut data);
        libresvip_core::io::write(path, out).at_path(path)?;
        Ok(())
    }
}
