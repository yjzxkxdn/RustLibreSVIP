//! Yuan77 project file (`y77`) — port of `libresvip.plugins.y77`.

pub mod generator;
pub mod model;
pub mod parser;
pub mod pinyin;
pub mod pitch;

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::model::reset_time_axis::reset_time_axis;
use libresvip_core::options::{
    enable_pitch_importation, select_single_track, static_tempo, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct Y77Plugin;

pub static PLUGIN: Y77Plugin = Y77Plugin;

impl SvsPlugin for Y77Plugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_Y77
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_Y77_VERSION
    }

    /// `InputOptions(EnablePitchImportationMixin, BaseModel)`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![enable_pitch_importation()]
    }

    /// `OutputOptions(SelectSingleTrackMixin, StaticTempoMixin)`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![static_tempo(), select_single_track()]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let y77_project: model::Y77Project = serde_json::from_slice(&bytes)?;
        parser::Y77Parser::new(options).parse_project(&y77_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let reset_required = project.song_tempo_list.len() != 1;
        let project = if reset_required {
            if project.song_tempo_list.is_empty() {
                return Err(LibreSvipError::msg("no tempo found"));
            }
            reset_time_axis(project, options.float("tempo", 120.0))?
        } else {
            project.clone()
        };
        let y77_project = generator::Y77Generator::new(options).generate_project(&project)?;
        let text = model::escape_non_ascii(&serde_json::to_string(&y77_project)?);
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
