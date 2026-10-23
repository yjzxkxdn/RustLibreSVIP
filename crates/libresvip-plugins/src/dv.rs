//! DeepVocal project file (`dv`) — port of `libresvip.plugins.dv`.
//!
//! The container is a little-endian binary structure built with `construct` in
//! Python; [`model`] reproduces the exact layout.

mod constants;
mod generator;
pub(crate) mod model;
mod parser;
pub(crate) mod pitch;

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct DeepVocalPlugin;

pub static PLUGIN: DeepVocalPlugin = DeepVocalPlugin;

impl SvsPlugin for DeepVocalPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_DV
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_DV_VERSION
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

    /// `OutputOptions(BaseModel)` — no fields.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let dv_project = model::parse_project(&bytes)?;
        parser::DeepVocalParser {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
        }
        .parse_project(&dv_project)
    }

    fn dump(&self, path: &Path, project: &Project, _options: &Options) -> Result<()> {
        let dv_project = generator::DeepVocalGenerator.generate_project(project)?;
        let bytes = model::build_project(&dv_project);
        libresvip_core::io::write(path, bytes).at_path(path)?;
        Ok(())
    }
}
