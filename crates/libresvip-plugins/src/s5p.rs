//! Synthesizer V Editor project (`.s5p`) plugin — port of
//! `libresvip.plugins.s5p`.

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::audio::audio_path_validator;

use crate::plugin_infos;

pub mod generator;
pub mod math_utils;
pub mod model;
pub mod parser;
pub mod synthv_pitch;

use generator::{OutputOptions, SynthVEditorGenerator};
use model::S5pProject;
use parser::{InputOptions, SynthVEditorParser};

pub struct S5pPlugin;

pub static PLUGIN: S5pPlugin = S5pPlugin;

impl SvsPlugin for S5pPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_S5P
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_S5P_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![enable_pitch_importation(), enable_instrumental_track_importation()]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let content = std::str::from_utf8(&bytes)
            .map_err(|error| LibreSvipError::Validation(format!("invalid UTF-8: {error}")))?;
        let raw: serde_json::Value = serde_json::from_str(content)?;
        let mut s5p_project: S5pProject = serde_json::from_value(raw.clone())?;
        // `audio_path_validator` only runs for an explicitly present field in
        // pydantic; a defaulted `filename` is left untouched.
        let filename_present = raw
            .get("instrumental")
            .and_then(|instrumental| instrumental.get("filename"))
            .map(|filename| !filename.is_null())
            .unwrap_or(false);
        if filename_present {
            s5p_project.instrumental.filename =
                resolve_audio_path(path, &s5p_project.instrumental.filename);
        }
        SynthVEditorParser::new(InputOptions::from_options(options))
            .parse_project(&s5p_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let s5p_project = SynthVEditorGenerator::new(OutputOptions::from_options(options))
            .generate_project(project)?;
        let json = serde_json::to_string(&s5p_project)?;
        let data = escape_non_ascii(&json).into_bytes();
        libresvip_core::io::write(path, data).at_path(path)?;
        Ok(())
    }
}

/// Python's `audio_path_validator` resolves a relative audio path against the
/// project file's directory and then calls `Path.resolve()`, which also makes
/// it absolute and collapses `.` / `..` components.
fn resolve_audio_path(project_path: &Path, filename: &str) -> String {
    let resolved = audio_path_validator(filename, Some(project_path));
    let path = Path::new(&resolved);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    normalize_path(&absolute).to_string_lossy().into_owned()
}

fn normalize_path(path: &Path) -> std::path::PathBuf {
    use std::path::Component;

    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let popped = result.pop();
                if !popped && !result.has_root() {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// Python's `json.dumps` defaults to `ensure_ascii=True`, so every non-ASCII
/// character (they can only occur inside strings) is written as a `\uXXXX`
/// escape, with astral characters as a surrogate pair.
fn escape_non_ascii(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    for character in json.chars() {
        if character.is_ascii() {
            result.push(character);
            continue;
        }
        let code = character as u32;
        if code <= 0xFFFF {
            result.push_str(&format!("\\u{code:04x}"));
        } else {
            let value = code - 0x1_0000;
            let high = 0xD800 + (value >> 10);
            let low = 0xDC00 + (value & 0x3FF);
            result.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
        }
    }
    result
}
