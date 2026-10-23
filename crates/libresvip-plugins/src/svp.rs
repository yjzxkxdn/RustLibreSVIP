//! SynthV Studio project (`.svp`) plugin — port of `libresvip.plugins.svp`.

use std::path::{Component, Path, PathBuf};

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub mod constants;
pub mod generator;
pub mod math_utils;
pub mod model;
pub mod options;
pub mod param_expression;
pub mod parser;
pub mod phoneme_utils;
pub mod synthv_pitch;
pub mod track_merge_utils;

use generator::SynthVGenerator;
use model::SVProject;
use options::{input_schema, output_schema, InputOptions, OutputOptions, VERSION_ABOVE_2_0_0};
use parser::SynthVParser;

pub struct SvpPlugin;

pub static PLUGIN: SvpPlugin = SvpPlugin;

impl SvsPlugin for SvpPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_SVP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_SVP_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        input_schema()
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        output_schema()
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let bytes = if bytes.last() == Some(&0) {
            &bytes[..bytes.len() - 1]
        } else {
            &bytes[..]
        };
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        let content = std::str::from_utf8(bytes)
            .map_err(|error| LibreSvipError::Validation(format!("invalid UTF-8: {error}")))?;
        let mut sv_project = parse_sv_project(content)?;
        sv_project.post_validate();
        resolve_audio_paths(path, &mut sv_project);
        let mut input_options = InputOptions::from_options(options);
        if input_options.instant {
            if let Some(instant_mode_enabled) = sv_project.instant_mode_enabled {
                input_options.instant = instant_mode_enabled;
            }
        }
        SynthVParser::new(input_options).parse_project(&sv_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = OutputOptions::from_options(options);
        let version_compatibility = output_options.version_compatibility;
        let sv_project = SynthVGenerator::new(output_options).generate_project(project)?;
        let json = serde_json::to_string(&sv_project)?;
        let mut data = escape_non_ascii(&json).into_bytes();
        if version_compatibility != VERSION_ABOVE_2_0_0 {
            data.push(0);
        }
        std::fs::write(path, data).at_path(path)?;
        Ok(())
    }
}

/// `audio_path_validator`: relative audio paths resolve against the project
/// file's directory.
fn resolve_audio_paths(project_path: &Path, project: &mut SVProject) {
    for track in &mut project.tracks {
        resolve_audio(project_path, &mut track.main_ref.audio);
        for group in &mut track.groups {
            resolve_audio(project_path, &mut group.audio);
        }
    }
}

fn resolve_audio(project_path: &Path, audio: &mut Option<model::SVAudio>) {
    let Some(audio) = audio else {
        return;
    };
    let path = Path::new(&audio.filename);
    if path.is_absolute() || audio.filename.is_empty() {
        return;
    }
    let base = project_path.parent().unwrap_or_else(|| Path::new(""));
    let joined = base.join(path);
    let absolute = if joined.is_absolute() {
        joined
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(&joined))
            .unwrap_or(joined)
    };
    audio.filename = normalize_path(&absolute).to_string_lossy().into_owned();
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// serde_json parses long decimal fractions with a fast algorithm that can be
/// off by one ULP. Quote every floating point token before parsing, then turn
/// the quoted values back into JSON numbers via Rust's correctly rounded
/// `str::parse::<f64>()`.
fn parse_sv_project(content: &str) -> Result<SVProject> {
    const SENTINEL: &str = "\u{1}PYF:";
    let quoted = quote_float_tokens(content);
    let mut value: serde_json::Value = serde_json::from_str(&quoted)?;
    restore_float_tokens(&mut value, SENTINEL);
    Ok(serde_json::from_value(value)?)
}

fn quote_float_tokens(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 32);
    let mut in_string = false;
    let mut escaped = false;
    let mut characters = text.char_indices().peekable();
    while let Some((_, character)) = characters.next() {
        if in_string {
            result.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            result.push(character);
            continue;
        }
        if character == '-' || character.is_ascii_digit() {
            let mut token = String::new();
            token.push(character);
            while let Some(&(_, next)) = characters.peek() {
                if next.is_ascii_digit() || matches!(next, '.' | 'e' | 'E' | '+' | '-') {
                    token.push(next);
                    characters.next();
                } else {
                    break;
                }
            }
            if token.contains('.') || token.contains('e') || token.contains('E') {
                result.push_str("\"\\u0001PYF:");
                result.push_str(&token);
                result.push('"');
            } else {
                result.push_str(&token);
            }
            continue;
        }
        result.push(character);
    }
    result
}

fn restore_float_tokens(value: &mut serde_json::Value, sentinel: &str) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(rest) = text.strip_prefix(sentinel) {
                if let Ok(parsed) = rest.parse::<f64>() {
                    if let Some(number) = serde_json::Number::from_f64(parsed) {
                        *value = serde_json::Value::Number(number);
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                restore_float_tokens(item, sentinel);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values_mut() {
                restore_float_tokens(item, sentinel);
            }
        }
        _ => {}
    }
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
