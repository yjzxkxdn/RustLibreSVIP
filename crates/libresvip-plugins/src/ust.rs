//! UST (UTAU sequence text) plugin — port of `libresvip.plugins.ust`.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_pitch_importation, select_single_track, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::supported_charset_names;

use crate::plugin_infos;

mod constants;
mod encode;
mod grammar;
mod interpolation;
pub(crate) mod math;
mod model;
mod pitch;
mod pitch1;
mod pitch2;
mod rdp;
mod resampling;
mod template;
mod ust_generator;
mod ust_parser;
mod vibrato;

pub struct UstPlugin;

pub static PLUGIN: UstPlugin = UstPlugin;

impl SvsPlugin for UstPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_UST
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_UST_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_pitch_importation(),
            OptionFieldDef::string("encoding", "Text encoding", "SHIFT_JIS")
                .with_choices(supported_charset_names()),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            select_single_track(),
            OptionFieldDef::float("version", "Version", 1.2)
                .with_description("UST file version"),
            OptionFieldDef::string("encoding", "Text encoding", "SHIFT_JIS")
                .with_choices(supported_charset_names()),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let encoding = options.str("encoding", "SHIFT_JIS");
        let import_pitch = options.bool("import_pitch", true);
        let bytes = std::fs::read(path).at_path(path)?;
        let text = encode::decode(&bytes, &encoding);
        let ust_project = grammar::parse_ust(&text)?;
        ust_parser::UstParser::new(import_pitch).parse_project(&ust_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let encoding = options.str("encoding", "SHIFT_JIS");
        let generator = ust_generator::UstGenerator {
            version: options.float("version", 1.2),
            track_index: options.int("track_index", -1),
            encoding: encoding.clone(),
        };
        let ust_project = generator.generate_project(project)?;
        let bytes = template::render_ust(&ust_project, &encoding)?;
        std::fs::write(path, bytes).at_path(path)?;
        Ok(())
    }
}

