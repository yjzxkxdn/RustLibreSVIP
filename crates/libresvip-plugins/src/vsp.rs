//! Vocalina Studio project (`.vsp`) plugin — port of the upstream
//! `experimental/vsp` format (the early Vocalina Studio binary project
//! format). Rust-port addition of an upstream plugin that the Python program
//! itself does not load yet (it lives outside `libresvip.plugins`); see the
//! README known-differences section.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{self, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod binary;
pub mod constants;
pub mod generator;
pub mod parser;

pub struct VspPlugin;

pub static PLUGIN: VspPlugin = VspPlugin;

impl SvsPlugin for VspPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VSP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VSP_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, ExtractEmbededAudioMixin, BaseModel)`,
    /// whose fields the pydantic MRO orders `extract_audio`, `import_pitch`,
    /// `import_instrumental_track`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            options::extract_embedded_audio(),
            options::enable_pitch_importation(),
            options::enable_instrumental_track_importation(),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::string(
            "default_singer_name",
            "Default Singer Name",
            "Default",
        )
        .with_description("Default singer name for tracks without a singer")]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let input_options = parser::InputOptions {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
            extract_audio: options.bool("extract_audio", false),
        };
        parser::VspParser::new(input_options, path).parse_project(&bytes)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = generator::OutputOptions {
            default_singer_name: options.str("default_singer_name", "Default").to_owned(),
        };
        let bytes = generator::VspGenerator::new(output_options).generate_project(project)?;
        libresvip_core::io::write(path, bytes).at_path(path)?;
        Ok(())
    }
}
