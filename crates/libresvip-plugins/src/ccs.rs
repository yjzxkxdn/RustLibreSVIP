//! CeVIO Creative Studio project (`.ccs`) plugin — port of
//! `libresvip.plugins.ccs`.

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod generator;
pub mod interval;
pub mod parser;
pub mod pyutil;
pub mod techno_speech;

pub struct CcsPlugin;

pub static PLUGIN: CcsPlugin = CcsPlugin;

impl SvsPlugin for CcsPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_CCS
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_CCS_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::string("default_singer_name", "Default Singer Name", "さとうささら"),
            OptionFieldDef::string("default_singer_id", "Default Voicebank ID", "A"),
            OptionFieldDef::string("default_singer_version", "Default Voicebank Version", "1.0.0"),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| LibreSvipError::Validation(format!("invalid UTF-8: {error}")))?;
        parser::parse_project(text, options)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let text = generator::generate_project(project, options)?;
        std::fs::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
