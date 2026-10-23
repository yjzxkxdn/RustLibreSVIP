//! VOX Factory project file (`vfp`) — port of `libresvip.plugins.vfp`.
//!
//! The `.vfp` container is a ZIP archive with `project.json` and optional
//! `resources/` audio payloads.

pub mod audio;
pub mod generator;
pub mod model;
pub mod parser;
pub mod zip;

use std::io::{Cursor, Read};
use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::model::reset_time_axis::reset_time_axis;
use libresvip_core::options::{
    enable_instrumental_track_importation, enable_pitch_importation, extract_embedded_audio,
    static_tempo, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct VfpPlugin;

pub static PLUGIN: VfpPlugin = VfpPlugin;

impl SvsPlugin for VfpPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VFP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VFP_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, ExtractEmbededAudioMixin, BaseModel)`.
    /// Pydantic reverses the MRO, so the fields are declared in reverse:
    /// `extract_audio`, `import_pitch`, `import_instrumental_track`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            extract_embedded_audio(),
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
        ]
    }

    /// `OutputOptions(StaticTempoMixin, BaseModel)`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![static_tempo()]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let project_json = read_zip_entry(&bytes, "project.json")?;
        let vox_project: model::VoxProject = serde_json::from_slice(&project_json)?;
        if options.bool("extract_audio", false) {
            extract_audio_resources(path, &bytes, &vox_project)?;
        }
        parser::VoxFactoryParser::new(options, path).parse_project(&vox_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let reset_required = project.song_tempo_list.len() != 1;
        let project = if reset_required {
            if project.song_tempo_list.is_empty() {
                return Err(LibreSvipError::msg("no tempo found"));
            }
            reset_time_axis(project, options.float("tempo", 120.0))?
        } else {
            project.clone()
        };
        let mut generator = generator::VOXFactoryGenerator::new(options);
        let vox_project = generator.generate_project(&project)?;
        let text = python_json_spaced(&serde_json::to_string(&vox_project)?);

        let mut audio_blobs: Vec<(&String, Vec<u8>)> = Vec::new();
        for (name, audio_path) in &generator.audio_paths {
            let bytes = std::fs::read(audio_path).at_path(audio_path)?;
            audio_blobs.push((name, bytes));
        }

        let mut entries: Vec<zip::ZipEntry<'_>> = Vec::new();
        entries.push(zip::ZipEntry {
            name: "project.json",
            data: text.as_bytes(),
            external_attr: 0o600u32 << 16,
            dos_date: zip::DOS_DATE,
            dos_time: zip::DOS_TIME,
        });
        entries.push(zip::ZipEntry {
            name: "resources/",
            data: &[],
            external_attr: ((0o40000 | 0o777) & 0xFFFF) << 16 | 0x10,
            dos_date: zip::DOS_DATE_DEFAULT,
            dos_time: 0,
        });
        let resource_names: Vec<String> = audio_blobs
            .iter()
            .map(|(name, _)| format!("resources/{name}"))
            .collect();
        for (index, (_, bytes)) in audio_blobs.iter().enumerate() {
            entries.push(zip::ZipEntry {
                name: &resource_names[index],
                data: bytes,
                external_attr: 0o600u32 << 16,
                dos_date: zip::DOS_DATE,
                dos_time: zip::DOS_TIME,
            });
        }
        let archive = zip::write_zip(&entries);
        std::fs::write(path, archive).at_path(path)?;
        Ok(())
    }
}

/// `json.dumps(..., ensure_ascii=False)` formatting: the default separators are
/// `", "` and `": "`, so a compact serde string gets one space after every
/// structural comma/colon (never inside strings).
fn python_json_spaced(compact: &str) -> String {
    let mut output = String::with_capacity(compact.len() + compact.len() / 8);
    let mut in_string = false;
    let mut escaped = false;
    for character in compact.chars() {
        output.push(character);
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if character == ',' || character == ':' {
            output.push(' ');
        }
    }
    output
}

fn read_zip_entry(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let mut archive = ::zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| LibreSvipError::Validation(format!("invalid zip archive: {error}")))?;
    let mut entry = archive
        .by_name(name)
        .map_err(|error| LibreSvipError::Validation(format!("missing {name}: {error}")))?;
    let mut content = Vec::new();
    entry
        .read_to_end(&mut content)
        .map_err(|error| LibreSvipError::Validation(format!("cannot read {name}: {error}")))?;
    Ok(content)
}

/// `VOXFactoryAudioClip.extract_audio` — write embedded `resources/<key>`
/// payloads next to the project file when `extract_audio` is enabled.
fn extract_audio_resources(
    path: &Path,
    bytes: &[u8],
    project: &model::VoxProject,
) -> Result<()> {
    for track in project.track_bank.values() {
        if track.type_ != "audio" {
            continue;
        }
        for clip in track.clip_bank.values() {
            let Some(key) = clip.source_audio_data_key.as_ref() else {
                continue;
            };
            if key.is_empty() {
                continue;
            }
            let target = audio_target_path(path, &clip.name, key);
            if target.exists() {
                continue;
            }
            let resource = read_zip_entry(bytes, &format!("resources/{key}"))?;
            std::fs::write(&target, resource).at_path(&target)?;
        }
    }
    Ok(())
}

fn audio_target_path(path: &Path, name: &str, source_audio_data_key: &str) -> std::path::PathBuf {
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    let mut target = base.join(name);
    let suffix = Path::new(source_audio_data_key)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.is_empty() {
        if suffix.is_empty() {
            target.set_extension("");
        } else {
            target.set_extension(suffix);
        }
    }
    target
}
