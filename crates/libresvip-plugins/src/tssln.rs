//! VoiSona project file (`.tssln`) plugin — port of
//! `libresvip.plugins.tssln`.
//!
//! A `.tssln` file is a JUCE `ValueTree` dump. `load` decodes the tree into the
//! pydantic-equivalent model in [`model`] and runs [`parser`]; `dump` builds the
//! tree from the internal [`Project`] with [`generator`] and serialises it with
//! [`value_tree`].

pub mod constants;
pub mod generator;
pub mod model;
pub mod parser;
pub mod pitch;
pub mod value_tree;

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct TsslnPlugin;

pub static PLUGIN: TsslnPlugin = TsslnPlugin;

impl SvsPlugin for TsslnPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_TSSLN
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_TSSLN_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, BaseModel)`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
        ]
    }

    /// `OutputOptions(BaseModel)`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let root = value_tree::parse(&bytes)?;
        let voisona_project = model::project_from_node(&root)?;
        check_project_version(&voisona_project.version_of_app_file_saved)?;
        parser::parse_project(&voisona_project, options)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let root = generator::generate_project(project, options)?;
        let bytes = value_tree::build(&root)?;
        std::fs::write(path, bytes).at_path(path)?;
        Ok(())
    }
}

/// `VoiSonaProject.version_validator`: reject versions older than `1.8`.
fn check_project_version(value: &str) -> Result<()> {
    if version_less_than(value, &[1, 8])? {
        return Err(LibreSvipError::Message(format!(
            "Unsupported project version {value}"
        )));
    }
    Ok(())
}

fn version_less_than(value: &str, target: &[u64]) -> Result<bool> {
    let components = parse_version(value).ok_or_else(|| {
        LibreSvipError::Validation(format!("invalid version string: {value}"))
    })?;
    let len = components.len().max(target.len());
    for index in 0..len {
        let left = components.get(index).copied().unwrap_or(0);
        let right = target.get(index).copied().unwrap_or(0);
        if left < right {
            return Ok(true);
        }
        if left > right {
            return Ok(false);
        }
    }
    Ok(false)
}

/// Numeric release components of a `packaging.version.Version`-style string.
fn parse_version(value: &str) -> Option<Vec<u64>> {
    let mut components = Vec::new();
    for part in value.split('.') {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return if components.is_empty() {
                None
            } else {
                Some(components)
            };
        }
        components.push(digits.parse().ok()?);
    }
    if components.is_empty() {
        None
    } else {
        Some(components)
    }
}
