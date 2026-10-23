//! `vspx` plugin — port of `libresvip.plugins.vspx`.
//!
//! VocalSharp projects are XML documents (`<VSPX><Version/><Project/>…`). The
//! writer reproduces the xsdata serializer output (`pretty_print` with tab
//! indentation, CDATA for text fields, empty elements expanded) including the
//! converter's post-processing regexes.

pub mod generator;
pub mod model;
pub mod music_math;
pub mod parser;
pub mod pitch_curve;
pub mod xml;

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct VocalSharpPlugin;

pub static PLUGIN: VocalSharpPlugin = VocalSharpPlugin;

impl SvsPlugin for VocalSharpPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VSPX
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VSPX_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, BaseModel)` — pydantic reverses the MRO.
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
        let vspx_project = xml::parse(&bytes)?;
        parser::VocalSharpParser::new(options).parse_project(&vspx_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = generator::OutputOptions::from_options(options);
        let mut generator = generator::VocalSharpGenerator::new(output_options);
        let vspx_project = generator.generate_project(project)?;
        let text = xml::render(&vspx_project);
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
