//! Piapro Studio project file (`ppsf`) — port of `libresvip.plugins.ppsf`.
//!
//! Two container variants exist:
//!
//! * the modern format: a ZIP archive holding `ppsf.json` (JSON model of the
//!   NT project) — see [`model`], [`parser`] and [`generator`];
//! * the legacy binary `PPSF` chunk format — **not ported yet** (it needs the
//!   construct-based `legacy_model` and a 40 KB byte-template generator). The
//!   `use_legacy_format` output option is exposed, but selecting it returns
//!   [`LibreSvipError::NotImplemented`].

pub(crate) mod audio;
pub(crate) mod generator;
pub(crate) mod interval_dict;
pub(crate) mod model;
pub(crate) mod parser;
pub(crate) mod util;
pub(crate) mod xsampa;

use std::io::{Cursor, Read};
use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct PpsfPlugin;

pub static PLUGIN: PpsfPlugin = PpsfPlugin;

/// Entry name of the JSON document inside the modern container.
pub const PPSF_JSON_ENTRY: &str = "ppsf.json";

impl SvsPlugin for PpsfPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_PPSF
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_PPSF_VERSION
    }

    /// `InputOptions(EnableInstrumentalTrackImportationMixin,
    /// EnablePitchImportationMixin, BaseModel)`.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            libresvip_core::options::enable_pitch_importation(),
            libresvip_core::options::enable_instrumental_track_importation(),
        ]
    }

    /// `OutputOptions(use_legacy_format=False)`.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::bool(
            "use_legacy_format",
            "Export as legacy format",
            false,
        )
        .with_description(
            "Export in the legacy binary PPSF format instead of the newer ZIP-based JSON format",
        )]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let content = std::fs::read(path).at_path(path)?;
        if content.starts_with(b"PK") {
            let text = read_ppsf_json(&content)?;
            let ppsf_project = model::PpsfProject::from_json(&text)?;
            parser::PiaproStudioNtParser::new(options).parse_project(ppsf_project)
        } else if content.starts_with(b"PPSF") {
            Err(LibreSvipError::NotImplemented)
        } else {
            Err(LibreSvipError::InvalidFileType)
        }
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        if options.bool("use_legacy_format", false) {
            return Err(LibreSvipError::NotImplemented);
        }
        let ppsf_project = generator::PiaproStudioGenerator::new(options).generate_project(project)?;
        let text = ppsf_project.to_json()?;
        let buffer = write_stored_zip(PPSF_JSON_ENTRY, text.as_bytes());
        std::fs::write(path, buffer).at_path(path)?;
        Ok(())
    }
}

/// Reads `ppsf.json` from an in-memory ZIP container.
fn read_ppsf_json(content: &[u8]) -> Result<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(content))
        .map_err(|error| LibreSvipError::Validation(format!("invalid zip archive: {error}")))?;
    let mut entry = archive
        .by_name(PPSF_JSON_ENTRY)
        .map_err(|error| LibreSvipError::Validation(format!("missing {PPSF_JSON_ENTRY}: {error}")))?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|error| LibreSvipError::Validation(format!("cannot read {PPSF_JSON_ENTRY}: {error}")))?;
    Ok(text)
}

fn write_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

/// Writes a single stored entry the way `zipfile.ZipFile(buffer, "w")` +
/// `zf.writestr(name, bytes)` does: no flags, no extra field, DOS timestamp,
/// `create_system = 0` and `external_attr = 0o600 << 16`.
fn write_stored_zip(name: &str, data: &[u8]) -> Vec<u8> {
    let (date, time) = crate::ps_project::zip_aes::dos_date_time();
    let crc = crc32fast::hash(data);
    let name_bytes = name.as_bytes();
    let size = data.len() as u32;
    let mut output = Vec::with_capacity(30 + name_bytes.len() + data.len() + 46 + name_bytes.len() + 22);
    write_u32(&mut output, 0x0403_4b50);
    write_u16(&mut output, 20);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u16(&mut output, time);
    write_u16(&mut output, date);
    write_u32(&mut output, crc);
    write_u32(&mut output, size);
    write_u32(&mut output, size);
    write_u16(&mut output, name_bytes.len() as u16);
    write_u16(&mut output, 0);
    output.extend_from_slice(name_bytes);
    output.extend_from_slice(data);
    let central_offset = output.len() as u32;
    write_u32(&mut output, 0x0201_4b50);
    write_u16(&mut output, 20);
    write_u16(&mut output, 20);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u16(&mut output, time);
    write_u16(&mut output, date);
    write_u32(&mut output, crc);
    write_u32(&mut output, size);
    write_u32(&mut output, size);
    write_u16(&mut output, name_bytes.len() as u16);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u32(&mut output, 0o600 << 16);
    write_u32(&mut output, 0);
    output.extend_from_slice(name_bytes);
    let central_size = output.len() as u32 - central_offset;
    write_u32(&mut output, 0x0605_4b50);
    write_u16(&mut output, 0);
    write_u16(&mut output, 0);
    write_u16(&mut output, 1);
    write_u16(&mut output, 1);
    write_u32(&mut output, central_size);
    write_u32(&mut output, central_offset);
    write_u16(&mut output, 0);
    output
}
