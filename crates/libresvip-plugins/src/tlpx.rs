//! TuneLab (`.tlpx`) plugin — port of `libresvip.plugins.tlpx`.
//!
//! `load` parses a CBOR document and `dump` writes one; the tree lives in
//! [`pyjson`] so the encoder can mirror `cbor2`'s defaults exactly.

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, EnumVariant, OptionFieldDef,
    Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod generator;
pub mod model;
pub mod music;
pub mod parser;
pub mod pyjson;

const VERSION_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "LEGACY",
        value: "0",
        title: "TLPX v0 (legacy)",
    },
    EnumVariant {
        name: "CURRENT",
        value: "1",
        title: "TLPX v1 (current)",
    },
];

pub struct TuneLabXPlugin;

pub static PLUGIN: TuneLabXPlugin = TuneLabXPlugin;

impl SvsPlugin for TuneLabXPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_TLPX
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_TLPX_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        // pydantic collects the mixin fields in reverse MRO order.
        vec![
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        let mut version_field = OptionFieldDef::enumerated(
            "version_compatibility",
            "TLPX version compatibility",
            "TuneLabVersionCompatibility",
            VERSION_VARIANTS,
            "CURRENT",
        )
        .with_description("Choose the version written when exporting TLPX.");
        // `str(TuneLabVersionCompatibility.CURRENT)` prints the integer value.
        version_field.default_repr = None;
        vec![version_field]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let value = pyjson::decode_cbor(&bytes)?;
        let tlp_project = model::TuneLabProject::from_json(&value)?;
        let input_options = parser::InputOptions {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
        };
        parser::TuneLabParser::new(input_options).parse_project(&tlp_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let raw_version = options.str("version_compatibility", "1");
        let numeric = raw_version.trim().parse::<i64>().map_err(|_| {
            LibreSvipError::Validation(format!("invalid version_compatibility: {raw_version}"))
        })?;
        let version = generator::TuneLabVersionCompatibility::from_value(numeric).ok_or_else(|| {
            LibreSvipError::Validation(format!("invalid version_compatibility: {raw_version}"))
        })?;
        let output_options = generator::OutputOptions {
            version_compatibility: version,
        };
        let tlp_project = generator::TuneLabGenerator::generate_project(project, &output_options)?;
        let bytes = pyjson::encode_cbor(&tlp_project.to_json())?;
        libresvip_core::io::write(path, bytes).at_path(path)?;
        Ok(())
    }
}
