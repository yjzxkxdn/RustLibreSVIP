//! Vogen project file (`vog`) — port of `libresvip.plugins.vog`.
//!
//! The `.vog` container is a ZIP archive with a single `chart.json` entry.

pub mod generator;
pub mod model;
pub mod parser;
pub mod pinyin;

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::model::reset_time_axis::reset_time_axis;
use libresvip_core::options::{static_tempo, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct VogPlugin;

pub static PLUGIN: VogPlugin = VogPlugin;

impl SvsPlugin for VogPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VOG
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VOG_VERSION
    }

    /// `InputOptions(BaseModel)` — no fields.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    /// `OutputOptions(StaticTempoMixin, BaseModel)` plus `singer_name`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            static_tempo(),
            OptionFieldDef::string("singer_name", "Singer name", "Doaz")
                .with_description("Please enter the singer's English name."),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let chart = model::read_chart_json(&bytes)?;
        let vogen_project: model::VogenProject = serde_json::from_slice(&chart)?;
        parser::VogenParser::new(options).parse_project(&vogen_project)
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
        let vogen_project = generator::VogenGenerator::new(options).generate_project(project_ref)?;
        let chart = model::to_json(&vogen_project)?;
        let archive = model::write_archive(chart.as_bytes());
        libresvip_core::io::write(path, archive).at_path(path)?;
        Ok(())
    }
}
