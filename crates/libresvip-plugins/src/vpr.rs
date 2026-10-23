//! `vpr` plugin — port of `libresvip.plugins.vpr`.
//!
//! A `.vpr` file (VOCALOID 5/6 "New Vocaloid Project") is a ZIP archive whose
//! `Project/sequence.json` entry holds the project, plus `Project/Audio/*` WAV
//! payloads. The JSON is written exactly like Python's
//! `json.dumps(model_dump(mode="json", exclude_none=True, by_alias=True),
//! ensure_ascii=False)`: field order from the pydantic models, `None` fields
//! dropped and `", "` / `": "` separators.

pub mod constants;
pub mod controllers;
pub mod generator;
pub mod model;
pub mod parser;
pub mod phonemes;
pub mod pitch;
pub mod zip;

use std::io::Read;
use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_breath_importation, enable_gender_importation, enable_instrumental_track_importation,
    enable_pitch_importation, enable_strength_importation, enable_volume_importation,
    extract_embedded_audio, EnumVariant, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub use model::language;

static VOCALOID_LANGUAGE_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "JAPANESE",
        value: "0",
        title: "日本語",
    },
    EnumVariant {
        name: "ENGLISH",
        value: "1",
        title: "English",
    },
    EnumVariant {
        name: "KOREAN",
        value: "2",
        title: "한국어",
    },
    EnumVariant {
        name: "SPANISH",
        value: "3",
        title: "Español",
    },
    EnumVariant {
        name: "SIMPLIFIED_CHINESE",
        value: "4",
        title: "简体中文",
    },
];

pub struct VprPlugin;

pub static PLUGIN: VprPlugin = VprPlugin;

impl SvsPlugin for VprPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VPR
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VPR_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, EnableVolumeImportationMixin,
    /// EnableBreathImportationMixin, EnableGenderImportationMixin,
    /// EnableStrengthImportationMixin, ExtractEmbededAudioMixin, BaseModel)` —
    /// pydantic collects mixin fields in reverse MRO order.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            extract_embedded_audio(),
            enable_strength_importation(),
            enable_gender_importation(),
            enable_breath_importation(),
            enable_volume_importation(),
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
        ]
    }

    /// `OutputOptions(BaseModel)` — declaration order.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        let mut language_field = OptionFieldDef::enumerated(
            "default_lang_id",
            "Default language",
            "VocaloidLanguage",
            VOCALOID_LANGUAGE_VARIANTS,
            "SIMPLIFIED_CHINESE",
        )
        .with_description("Default language id of voicebank and notes");
        // `str(VocaloidLanguage.SIMPLIFIED_CHINESE)` prints the integer value.
        language_field.default_repr = None;
        vec![
            OptionFieldDef::bool("is_ai_singer", "Is AI Singer", false)
                .with_description("The default voicebank is AI singer or not"),
            language_field,
            OptionFieldDef::string("default_comp_id", "Default Comp ID", "BL8CEAM5N4XN3LFK")
                .with_description("Default comp_id of voicebank"),
            OptionFieldDef::string(
                "default_singer_name",
                "Default Singer Name",
                "Luo_Tianyi_Wan",
            )
            .with_description("Default singer name of voicebank"),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let sequence = read_zip_entry(&bytes, "Project/sequence.json")?;
        let vpr_project: model::VocaloidProject = serde_json::from_slice(&sequence)?;
        if options.bool("extract_audio", false) {
            extract_audio(path, &bytes, &vpr_project)?;
        }
        parser::VocaloidParser::new(options, path).parse_project(&vpr_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = generator::OutputOptions::from_options(options);
        let mut generator = generator::VocaloidGenerator::new(output_options);
        let vpr_project = generator.generate_project(project)?;
        let compact = serde_json::to_string(&vpr_project)?;
        let text = python_json_spaced(&compact);

        // `zipfile.ZipFile.writestr` for str data sets `0o600 << 16`.
        const FILE_EXTERNAL_ATTR: u32 = 0o600u32 << 16;
        let mut audio_blobs: Vec<(String, Vec<u8>)> = Vec::new();
        for (wav_name, wav_path) in &generator.wav_paths {
            let data = libresvip_core::io::read(wav_path).at_path(wav_path)?;
            audio_blobs.push((format!("Project/Audio/{wav_name}"), data));
        }

        let mut entries: Vec<zip::ZipEntry<'_>> = Vec::with_capacity(1 + audio_blobs.len());
        entries.push(zip::ZipEntry {
            name: "Project/sequence.json",
            data: text.as_bytes(),
            external_attr: FILE_EXTERNAL_ATTR,
            dos_date: zip::DOS_DATE,
            dos_time: zip::DOS_TIME,
        });
        for (name, data) in &audio_blobs {
            entries.push(zip::ZipEntry {
                name,
                data,
                external_attr: FILE_EXTERNAL_ATTR,
                dos_date: zip::DOS_DATE,
                dos_time: zip::DOS_TIME,
            });
        }
        let archive = zip::write_zip(&entries);
        libresvip_core::io::write(path, archive).at_path(path)?;
        Ok(())
    }
}

/// Reads one entry from an in-memory ZIP archive.
fn read_zip_entry(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ::zip::ZipArchive::new(cursor)
        .map_err(|error| LibreSvipError::msg(format!("invalid zip archive: {error}")))?;
    let mut file = archive
        .by_name(name)
        .map_err(|error| LibreSvipError::msg(format!("missing archive entry {name}: {error}")))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|error| {
        LibreSvipError::msg(format!("cannot read archive entry {name}: {error}"))
    })?;
    Ok(data)
}

/// `VocaloidWav.extract_audio` — writes `Project/Audio/<name>` next to the
/// project when the audio file is missing (`extract_audio` option).
fn extract_audio(path: &Path, bytes: &[u8], vpr_project: &model::VocaloidProject) -> Result<()> {
    let mut archive = ::zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| LibreSvipError::msg(format!("invalid zip archive: {error}")))?;
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    for track in &vpr_project.tracks {
        let model::VocaloidTrack::Audio(track) = track else {
            continue;
        };
        for part in &track.parts {
            let Some(wav) = &part.wav else { continue };
            let destination = parent.join(wav.original_name.as_deref().unwrap_or(&wav.name));
            if destination.exists() {
                continue;
            }
            let archive_path = format!("Project/Audio/{}", wav.name);
            let mut file = archive.by_name(&archive_path).map_err(|error| {
                LibreSvipError::msg(format!("missing archive entry {archive_path}: {error}"))
            })?;
            let mut data = Vec::new();
            file.read_to_end(&mut data).map_err(|error| {
                LibreSvipError::msg(format!("cannot read archive entry {archive_path}: {error}"))
            })?;
            libresvip_core::io::write(&destination, data).at_path(&destination)?;
        }
    }
    Ok(())
}

/// `json.dumps(..., ensure_ascii=False)` formatting: the default separators are
/// `", "` and `": "`, so a compact serde string gets one space after every
/// structural comma/colon (never inside strings).
pub fn python_json_spaced(compact: &str) -> String {
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
