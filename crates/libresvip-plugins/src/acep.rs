//! ACE Studio project file plugin (`.acep` / `.acet`) — port of
//! `libresvip.plugins.acep`.
//!
//! The project is a zstd-compressed JSON (or CBOR) payload wrapped in either a
//! JSON container (ACE Studio ≤ 2.0.6) or the `ACEP2` binary container
//! (ACE Studio ≥ 2.0.7). `.acet` files are zip archives holding one `.acep`.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::audio::audio_path_validator;

use crate::plugin_infos;

mod base_pitch_curve;
mod color_pool;
mod generator;
mod interval;
mod io;
mod model;
mod music_math;
mod options;
mod parser;
mod singers;

use generator::AceGenerator;
use model::AcepProject;
use options::{InputOptions, OutputOptions};
use parser::AceParser;

pub struct AcepPlugin;

pub static PLUGIN: AcepPlugin = AcepPlugin;

impl SvsPlugin for AcepPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_ACEP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_ACEP_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        options::input_option_fields()
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        options::output_option_fields()
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let content = if path
            .extension()
            .map(|extension| extension.eq_ignore_ascii_case("acet"))
            .unwrap_or(false)
        {
            match io::read_acet_entry(&bytes)? {
                Some(content) => content,
                None => bytes,
            }
        } else {
            bytes
        };
        let value = io::decompress_project(&content)?;
        let mut acep_project: AcepProject = serde_json::from_value(value)?;
        // `audio_path_validator` resolves relative audio paths against the
        // project file (pydantic runs it while validating `AcepAudioPattern`).
        for track in acep_project.tracks.iter_mut() {
            if let model::AcepTrack::Audio(audio_track) = track {
                for pattern in audio_track.patterns.iter_mut() {
                    pattern.path = audio_path_validator(&pattern.path, Some(path));
                }
            }
        }
        AceParser::new(InputOptions::from_options(options)).parse_project(&mut acep_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let options = OutputOptions::from_options(options);
        let acep_project = AceGenerator::new(options.clone()).generate_project(project)?;
        let bytes = io::compress_project(&acep_project, options.serialization)?;
        std::fs::write(path, bytes).at_path(path)?;
        Ok(())
    }
}

/// Differential harness against the Python reference implementation.
///
/// Point `tmp_acep/cases/*.acep` (fixtures to load) and `tmp_acep/dumps/*.json`
/// (projects to dump) at a scratch directory to regenerate `tmp_acep/rs/`; the
/// companion Python script writes `tmp_acep/py/` and the two trees are compared
/// with `diff`. Skipped when the directory is absent, like `ust.rs`.
#[cfg(test)]
mod differential {
    use super::*;
    use libresvip_core::options::OptionValue;

    fn options(pairs: &[(&str, OptionValue)]) -> Options {
        let mut options = Options::new();
        for (key, value) in pairs {
            options.insert(*key, value.clone());
        }
        options
    }

    fn case_options(label: &str) -> Options {
        match label {
            "keep" => options(&[
                ("keep_all_pronunciation", OptionValue::Bool(true)),
                ("curve_sample_interval", OptionValue::Int(0)),
                ("import_gender", OptionValue::Bool(false)),
                ("breath", OptionValue::Int(200)),
                ("split_threshold", OptionValue::Int(0)),
            ]),
            "norm" => options(&[
                (
                    "breath_normalization",
                    OptionValue::Str("minmax,0,10,1,0.2".to_owned()),
                ),
                (
                    "tension_normalization",
                    OptionValue::Str("zscore,0,10,0.5,-0.1".to_owned()),
                ),
                (
                    "energy_normalization",
                    OptionValue::Str("minmax,0,10,1,0".to_owned()),
                ),
                ("energy_coefficient", OptionValue::Float(0.3)),
                ("map_strength_info", OptionValue::Str("energy".to_owned())),
                ("lyric_language", OptionValue::Str("JPN".to_owned())),
                (
                    "default_consonant_length",
                    OptionValue::Float(0.05),
                ),
            ]),
            "tension" => options(&[
                ("import_energy", OptionValue::Bool(false)),
                ("import_tension", OptionValue::Bool(true)),
                ("map_strength_info", OptionValue::Str("tension".to_owned())),
                ("curve_sample_interval", OptionValue::Int(10)),
                ("lyric_language", OptionValue::Str("ENG".to_owned())),
            ]),
            "energy" => options(&[
                ("import_tension", OptionValue::Bool(false)),
                ("import_energy", OptionValue::Bool(true)),
                ("import_breath", OptionValue::Bool(false)),
                ("import_pitch", OptionValue::Bool(false)),
                ("keep_all_pronunciation", OptionValue::Bool(true)),
                ("serialization", OptionValue::Str("cbor".to_owned())),
            ]),
            _ => Options::new(),
        }
    }

    const LABELS: [&str; 5] = ["default", "keep", "norm", "tension", "energy"];

    #[test]
    fn acep_differential() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp_acep");
        if !root.is_dir() {
            return;
        }
        let output = root.join("rs");
        std::fs::create_dir_all(&output).unwrap();
        for entry in std::fs::read_dir(&output).unwrap() {
            let _ = std::fs::remove_file(entry.unwrap().path());
        }
        for entry in std::fs::read_dir(root.join("cases")).unwrap() {
            let path = entry.unwrap().path();
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            for label in LABELS {
                let target = output.join(format!("{stem}.load.{label}.json"));
                match PLUGIN.load(&path, &case_options(label)) {
                    Ok(project) => std::fs::write(
                        target,
                        serde_json::to_vec(&project).unwrap(),
                    )
                    .unwrap(),
                    Err(_) => std::fs::write(target.with_extension("err"), "ERR").unwrap(),
                }
            }
        }
        for entry in std::fs::read_dir(root.join("dumps")).unwrap() {
            let path = entry.unwrap().path();
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let project: Project = serde_json::from_slice(&bytes).unwrap();
            for label in LABELS {
                let target = output.join(format!("{stem}.dump.{label}.acep"));
                if PLUGIN.dump(&target, &project, &case_options(label)).is_err() {
                    std::fs::write(target, "ERR").unwrap();
                }
            }
        }
    }
}
