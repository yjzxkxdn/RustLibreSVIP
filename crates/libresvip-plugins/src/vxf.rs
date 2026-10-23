//! VX-β (`.vxf`) plugin — port of `libresvip.plugins.vxf`.
//!
//! The file format is a little binary "UMP clip" container described by a
//! `construct` schema in the Python implementation; [`binary`] re-implements the
//! parser and serializer by hand.

use std::path::Path;

use libresvip_core::core::constants::{DEFAULT_BPM, TICKS_IN_BEAT};
use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{enable_pitch_importation, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod binary;
pub mod generator;
pub mod model;
pub mod music;
pub mod parser;
pub mod pyjson;

pub struct VxBetaPlugin;

pub static PLUGIN: VxBetaPlugin = VxBetaPlugin;

impl SvsPlugin for VxBetaPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VXF
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VXF_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_pitch_importation(),
            OptionFieldDef::float("default_bpm", "Default BPM", DEFAULT_BPM)
                .with_description("Used when no BPM information is found in the MIDI file."),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::int("ticks_per_beat", "Ticks per beat", TICKS_IN_BEAT)
            .with_description(
                "Also known as parts per quarter, ticks per quarter note, the number of pulses \
                 per quarter note. This setting should not be changed unless you know what it is.",
            )]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let vx_file = binary::VxFile::parse(&bytes)?;
        let input_options = parser::InputOptions {
            import_pitch: options.bool("import_pitch", true),
            default_bpm: options.float("default_bpm", DEFAULT_BPM),
        };
        parser::VxBetaParser::new(input_options).parse_project(&vx_file)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = generator::OutputOptions {
            ticks_per_beat: options.int("ticks_per_beat", TICKS_IN_BEAT),
        };
        let vx_file = generator::generate_project(project, &output_options)?;
        let bytes = vx_file.build()?;
        libresvip_core::io::write(path, bytes).at_path(path)?;
        Ok(())
    }
}
