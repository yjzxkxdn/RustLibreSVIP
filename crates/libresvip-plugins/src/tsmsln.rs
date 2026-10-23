//! VoiSona Mobile project file (`.tsmsln`) plugin — port of
//! `libresvip.plugins.tsmsln`.
//!
//! The container is the same JUCE `ValueTree` dump as `.tssln`, only the root
//! node and the model layout differ. `load` decodes the tree into the
//! pydantic-equivalent model in [`model`] and runs [`parser`]; `dump` builds the
//! tree from the internal [`Project`] with [`generator`] and serialises it with
//! [`value_tree`].

pub mod constants;
pub mod generator;
pub mod model;
pub mod parser;
pub mod pitch;
pub mod value_tree;

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{enable_pitch_importation, select_single_track, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct TsmslnPlugin;

pub static PLUGIN: TsmslnPlugin = TsmslnPlugin;

impl SvsPlugin for TsmslnPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_TSMSLN
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_TSMSLN_VERSION
    }

    /// `InputOptions(EnablePitchImportationMixin, BaseModel)`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![enable_pitch_importation()]
    }

    /// `OutputOptions(SelectSingleTrackMixin, BaseModel)`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![select_single_track()]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let root = value_tree::parse(&bytes)?;
        let voisona_project = model::project_from_node(&root)?;
        parser::parse_project(&voisona_project, options)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let root = generator::generate_project(project, options)?;
        let bytes = value_tree::build(&root)?;
        std::fs::write(path, bytes).at_path(path)?;
        Ok(())
    }
}
