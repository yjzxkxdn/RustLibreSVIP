//! NIAONiao project file (`nn`) — port of `libresvip.plugins.nn`.

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
use libresvip_core::utils::text::to_unicode;

use crate::plugin_infos;

pub struct NnPlugin;

pub static PLUGIN: NnPlugin = NnPlugin;

impl SvsPlugin for NnPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_NN
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_NN_VERSION
    }

    /// `InputOptions(EnablePitchImportationMixin, BaseModel)`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![enable_pitch_importation()]
    }

    /// `OutputOptions(SelectSingleTrackMixin, StaticTempoMixin, BaseModel)` plus
    /// the plugin's own `version` field. Pydantic orders the mixins in reverse
    /// MRO: `tempo`, `track_index`, then `version`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            static_tempo(),
            select_single_track(),
            OptionFieldDef::int("version", "Version", 19)
                .with_description("Version of NIAONiao project file"),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let text = to_unicode(&bytes);
        let nn_project = parser::parse_text(&text)?;
        parser::NiaoNiaoParser::new(options).parse_project(&nn_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let reset_required = project.song_tempo_list.len() != 1;
        // Borrow the project when no reset is needed instead of cloning
        // the whole thing just to unify the branch type.
        let reset;
        let project_ref: &Project = if reset_required {
            if project.song_tempo_list.is_empty() {
                return Err(LibreSvipError::msg("no tempo found"));
            }
            reset = reset_time_axis(project.clone(), options.float("tempo", 120.0))?;
            &reset
        } else {
            &project
        };
        let nn_project = generator::NiaoniaoGenerator::new(options).generate_project(project_ref)?;
        let text = model::render(&nn_project);
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
