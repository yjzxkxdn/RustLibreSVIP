//! AISingers Studio project (`.aisp`) plugin — port of `libresvip.plugins.aisp`.

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod generator;
pub mod model;
pub mod parser;
pub mod pitch;
pub mod pyutil;

pub struct AiSingersPlugin;

pub static PLUGIN: AiSingersPlugin = AiSingersPlugin;

impl SvsPlugin for AiSingersPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_AISP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_AISP_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        // `class InputOptions(EnablePitchImportationMixin,
        // EnableInstrumentalTrackImportationMixin, BaseModel)`: pydantic
        // collects mixin fields in reverse MRO order.
        vec![
            enable_instrumental_track_importation(),
            enable_pitch_importation(),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let content = std::str::from_utf8(&bytes)
            .map_err(|error| LibreSvipError::Validation(format!("invalid UTF-8: {error}")))?;
        let (head, body) = match content.split_once('\n') {
            Some((head, body)) => (head.trim(), body.trim()),
            None => (content.trim(), ""),
        };
        let head: model::AisProjectHead = serde_json::from_str(head)?;
        let body: model::AisProjectBody = serde_json::from_str(body)?;
        parser::AisParser::new(options).parse_project(&head, &body)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let content = generator::generate_project(project, options)?;
        std::fs::write(path, content.as_bytes()).at_path(path)?;
        Ok(())
    }
}
