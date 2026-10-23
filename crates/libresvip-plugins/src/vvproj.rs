//! VOICEVOX Project (`.vvproj`) plugin — port of `libresvip.plugins.vvproj`.
//!
//! The generator embeds fresh UUIDs, so the reference format is only
//! deterministic after normalising them.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{enable_pitch_importation, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::to_unicode;

use crate::plugin_infos;

pub mod generator;
pub mod model;
pub mod music;
pub mod parser;
pub mod pyjson;

pub struct VoiceVoxPlugin;

pub static PLUGIN: VoiceVoxPlugin = VoiceVoxPlugin;

impl SvsPlugin for VoiceVoxPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VVPROJ
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VVPROJ_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![enable_pitch_importation()]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let text = to_unicode(&bytes);
        let value = pyjson::parse(&text)?;
        let voicevox_project = model::VoiceVoxProject::from_json(&value)?;
        let input_options = parser::InputOptions {
            import_pitch: options.bool("import_pitch", true),
        };
        parser::parse_project(&voicevox_project, &input_options)
    }

    fn dump(&self, path: &Path, project: &Project, _options: &Options) -> Result<()> {
        let voicevox_project =
            generator::generate_project(project, &generator::OutputOptions)?;
        let text = pyjson::dumps(&voicevox_project.to_json(), true, true);
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
