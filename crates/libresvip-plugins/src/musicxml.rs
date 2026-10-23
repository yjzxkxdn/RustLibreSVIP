//! MusicXML plugin — port of `libresvip.plugins.musicxml`.
//!
//! Reads both plain MusicXML (`.musicxml`, `.xml`) and compressed `.mxl`
//! archives (via `META-INF/container.xml`), and writes MusicXML 4.0 partwise
//! scores with the same serialization as the Python/sxdata writer.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

mod generator;
mod music;
mod options;
mod parser;
mod xml;

pub struct MusicXmlPlugin;

pub static PLUGIN: MusicXmlPlugin = MusicXmlPlugin;

impl SvsPlugin for MusicXmlPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_MUSICXML
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_MUSICXML_VERSION
    }

    fn readable(&self) -> bool {
        true
    }

    fn writable(&self) -> bool {
        true
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        options::input_options()
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let input_options = options::InputOptions::from_options(options);
        parser::parse_project(
            &bytes,
            &path.display().to_string(),
            &input_options,
        )
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let generator = generator::MusicXmlGenerator::new(options);
        let score = generator.generate_project(project);
        let text = xml::render_document(&score);
        libresvip_core::io::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}
