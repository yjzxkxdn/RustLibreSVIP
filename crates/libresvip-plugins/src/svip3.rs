//! Netease Cloud Music XStudio project file (`svip3`) — port of
//! `libresvip.plugins.svip3`.
//!
//! The project file is a protobuf message; see [`model`] for the hand-written
//! codec and `libresvip/res/protos/svip3.proto` for the schema.

pub mod constants;
pub mod generator;
pub mod model;
pub mod parser;
pub mod singers;

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct Svip3Plugin;

pub static PLUGIN: Svip3Plugin = Svip3Plugin;

impl SvsPlugin for Svip3Plugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_SVIP3
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_SVIP3_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, BaseModel)` — pydantic's `model_fields`
    /// order is `import_pitch`, `import_instrumental_track`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            libresvip_core::options::enable_pitch_importation(),
            libresvip_core::options::enable_instrumental_track_importation(),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let svip3_project = model::Svip3Project::from_binary(&bytes)?;
        parser::Svip3Parser::new(options).parse_project(svip3_project)
    }

    fn dump(&self, path: &Path, project: &Project, _options: &Options) -> Result<()> {
        let svip3_project = generator::Svip3Generator::new().generate_project(project)?;
        libresvip_core::io::write(path, svip3_project.to_binary()).at_path(path)?;
        Ok(())
    }
}
