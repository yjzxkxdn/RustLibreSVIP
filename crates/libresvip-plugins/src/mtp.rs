//! Muta project file (`mtp`) — port of `libresvip.plugins.mtp`.
//!
//! The container is a little-endian binary structure built with `construct` in
//! Python; [`model`] reproduces the exact layout, including the `\n` separators
//! of `muta_prefixed_array`.

mod generator;
pub(crate) mod model;
mod parser;

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct MutaPlugin;

pub static PLUGIN: MutaPlugin = MutaPlugin;

impl SvsPlugin for MutaPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_MTP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_MTP_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, BaseModel)`: pydantic collects the mixin
    /// fields in reverse declaration order.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
        ]
    }

    /// `OutputOptions(BaseModel)` plus `default_singer_name`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::string(
            "default_singer_name",
            "Default Singer Name",
            "嫣汐",
        )
        .with_description("The default singer name to use for all tracks")]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let muta_project = model::parse_project(&bytes)?;
        parser::MutaParser {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
        }
        .parse_project(&muta_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let generator = generator::MutaGenerator {
            default_singer_name: options.str("default_singer_name", "嫣汐"),
        };
        let muta_project = generator.generate_project(project)?;
        let bytes = model::build_project(&muta_project)?;
        std::fs::write(path, bytes).at_path(path)?;
        Ok(())
    }
}
