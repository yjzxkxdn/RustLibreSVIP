//! `xvsq` plugin — port of `libresvip.plugins.xvsq`.
//!
//! Cadencii project files (`.xvsq`) are a plain XML serialisation of the
//! Cadencii `VsqFileEx` model.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_breath_importation, enable_gender_importation, enable_instrumental_track_importation,
    enable_pitch_importation, enable_strength_importation, enable_volume_importation,
    OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod generator;
pub mod parser;

pub struct XvsqPlugin;

pub static PLUGIN: XvsqPlugin = XvsqPlugin;

impl SvsPlugin for XvsqPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_XVSQ
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_XVSQ_VERSION
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
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::bool("pretty_xml", "Pretty XML", true)
            .with_description("Whether to output pretty XML")]
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
        };
        parser::parse_project(&bytes, &input_options)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = generator::OutputOptions {
            pretty_xml: options.bool("pretty_xml", true),
        };
        let text = generator::generate_project(project, &output_options)?;
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
