//! PocketSinger project file (`ps_project`) — port of
//! `libresvip.plugins.ps_project`.
//!
//! The container is a ZIP archive whose members are encrypted with WinZip AES
//! (AES-256, AE-2). Python relies on the external `zipctl` package; the reader
//! and writer live in [`zip_aes`] here, on top of the primitives in [`crypto`].

pub(crate) mod crypto;
pub(crate) mod generator;
pub(crate) mod model;
pub(crate) mod parser;
pub(crate) mod zip_aes;

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::model::reset_time_axis::reset_time_axis;
use libresvip_core::options::{EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct PocketSingerPlugin;

pub static PLUGIN: PocketSingerPlugin = PocketSingerPlugin;

/// `POCKET_SIGER_PASSWORD` (sic) — the archive password.
const POCKET_SINGER_PASSWORD: &[u8] = b"a022ab39cb3b7b1de92ee441978c9e08";

/// `PocketSingerLyricsLanguage` members.
pub static LYRICS_LANGUAGES: &[EnumVariant] = &[
    EnumVariant {
        name: "CHINESE",
        value: "ch",
        title: "Chinese",
    },
    EnumVariant {
        name: "JAPANESE",
        value: "jp",
        title: "Japanese",
    },
    EnumVariant {
        name: "ENGLISH",
        value: "en",
        title: "English",
    },
];

impl SvsPlugin for PocketSingerPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_PS_PROJECT
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_PS_PROJECT_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, ExtractEmbededAudioMixin, BaseModel)` —
    /// pydantic's `model_fields` order is `extract_audio`, `import_pitch`,
    /// `import_instrumental_track`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            libresvip_core::options::extract_embedded_audio(),
            libresvip_core::options::enable_pitch_importation(),
            libresvip_core::options::enable_instrumental_track_importation(),
        ]
    }

    /// `OutputOptions(StaticTempoMixin, BaseModel)` plus `lyric_language` and
    /// `title`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            libresvip_core::options::static_tempo(),
            OptionFieldDef::enumerated(
                "lyric_language",
                "Lyrics language",
                "PocketSingerLyricsLanguage",
                LYRICS_LANGUAGES,
                "CHINESE",
            ),
            OptionFieldDef::string("title", "Title of the project", "New Project"),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let content = libresvip_core::io::read(path).at_path(path)?;
        let entries = zip_aes::read_archive(&content, POCKET_SINGER_PASSWORD)?;
        let config = find_entry(&entries, "config.json")
            .ok_or_else(|| LibreSvipError::Validation("missing config.json".to_owned()))?;
        let metadata = model::PocketSingerMetadata::from_json(&config.data)
            .map_err(|error| LibreSvipError::Validation(format!("invalid config.json: {error}")))?;
        if options.bool("extract_audio", false) {
            if let Some(bgm_info) = &metadata.bgm_info {
                let target = path.parent().unwrap_or(path).join(format!(
                    "{}.{}",
                    bgm_info.display_name, bgm_info.file_type
                ));
                if !target.exists() {
                    let archive_name = format!("{}.{}", bgm_info.file_name, bgm_info.file_type);
                    if let Some(audio_entry) = find_entry(&entries, &archive_name) {
                        libresvip_core::io::write(&target, &audio_entry.data).at_path(&target)?;
                    }
                }
            }
        }
        let ace = find_entry(&entries, &metadata.ace_file_name).ok_or_else(|| {
            LibreSvipError::Validation(format!(
                "missing {} in archive",
                metadata.ace_file_name
            ))
        })?;
        let project = model::PocketSingerProject::from_json(&ace.data).map_err(|error| {
            LibreSvipError::Validation(format!("invalid {}: {error}", metadata.ace_file_name))
        })?;
        parser::PocketSingerParser::new(options, path, project).parse_project()
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let reset_project;
        let project = if project.song_tempo_list.len() != 1 {
            reset_project = reset_time_axis(project.clone(), options.float("tempo", 120.0))?;
            &reset_project
        } else {
            project
        };
        let mut generator = generator::PocketSingerGenerator::new(options);
        let metadata = generator.generate_project(project)?;
        let config_json = metadata.to_json()?;
        let mut entries: Vec<(String, Vec<u8>)> = vec![
            ("config.json".to_owned(), config_json.into_bytes()),
            (metadata.ace_file_name.clone(), generator.buffer.clone()),
        ];
        for (audio_name, audio_path) in &generator.audio_paths {
            let data = libresvip_core::io::read(audio_path).at_path(audio_path)?;
            entries.push((audio_name.clone(), data));
        }
        let entry_refs: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice()))
            .collect();
        let buffer = zip_aes::write_archive(&entry_refs, POCKET_SINGER_PASSWORD)?;
        libresvip_core::io::write(path, buffer).at_path(path)?;
        Ok(())
    }
}

fn find_entry<'a>(entries: &'a [zip_aes::ZipEntry], name: &str) -> Option<&'a zip_aes::ZipEntry> {
    entries.iter().find(|entry| entry.name == name)
}
