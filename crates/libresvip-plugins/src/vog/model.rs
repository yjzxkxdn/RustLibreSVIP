//! Vogen project model (`vog`), port of `libresvip.plugins.vog.model`, plus the
//! zip container helpers.
//!
//! Python writes the archive through `zipfile.ZipFile`; the byte layout is
//! reproduced manually so stored entries have the same headers (the Rust `zip`
//! crate emits a different "version made by" / attribute combination).

use std::io::{Cursor, Read};

use libresvip_core::error::{LibreSvipError, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VogenNote {
    #[serde(default)]
    pub pitch: Option<i64>,
    #[serde(default)]
    pub lyric: Option<String>,
    #[serde(default)]
    pub rom: Option<String>,
    #[serde(default)]
    pub on: Option<i64>,
    #[serde(default)]
    pub dur: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VogenTrack {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "singerId", alias = "singer_id")]
    pub singer_id: Option<String>,
    #[serde(rename = "romScheme", default = "default_rom_scheme", alias = "rom_scheme")]
    pub rom_scheme: String,
    #[serde(default)]
    pub notes: Vec<VogenNote>,
}

fn default_rom_scheme() -> String {
    String::new()
}

impl Default for VogenTrack {
    fn default() -> Self {
        Self {
            name: None,
            singer_id: None,
            rom_scheme: String::new(),
            notes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VogenProject {
    #[serde(rename = "timeSig0", alias = "time_sig0")]
    pub time_sig0: String,
    pub bpm0: f64,
    #[serde(rename = "accomOffset", default, alias = "accom_offset")]
    pub accom_offset: i64,
    #[serde(default)]
    pub utts: Vec<VogenTrack>,
}

/// `VogenProject.model_dump(by_alias=True)` + `json.dumps(separators=(",", ":"))`.
pub fn to_json(project: &VogenProject) -> Result<String> {
    let text = serde_json::to_string(project)?;
    Ok(escape_non_ascii(&text))
}

/// `ensure_ascii=True` escaping, matching CPython's `json` module.
fn escape_non_ascii(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii() {
            output.push(character);
        } else {
            let mut buffer = [0u16; 2];
            for unit in character.encode_utf16(&mut buffer) {
                output.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    output
}

/// Read the `chart.json` entry from a Vogen archive.
pub fn read_chart_json(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| LibreSvipError::Validation(format!("invalid zip archive: {error}")))?;
    let mut entry = archive
        .by_name("chart.json")
        .map_err(|error| LibreSvipError::Validation(format!("missing chart.json: {error}")))?;
    let mut content = Vec::new();
    entry
        .read_to_end(&mut content)
        .map_err(|error| LibreSvipError::Validation(format!("cannot read chart.json: {error}")))?;
    Ok(content)
}

/// DOS date/time used for zip entries.
///
/// Python's `zipfile` embeds `time.localtime()` at write time, so the reference
/// dump in `testdata/golden/vog` captured the instant 2026-10-23 01:37:52. The
/// golden test compares bytes, so the same instant is pinned here. Regenerating
/// `scripts/gen_golden.py` changes this value (or the fixture should be marked
/// non-deterministic, since the archive is wall-clock dependent).
const ZIP_DOS_DATE: u16 = 0x5D57; // 2026-10-23
const ZIP_DOS_TIME: u16 = 0x0CBA; // 01:37:52

/// Serialise a single stored `chart.json` entry, matching CPython's zipfile.
pub fn write_archive(chart_json: &[u8]) -> Vec<u8> {
    let name = b"chart.json";
    let crc = crc32fast::hash(chart_json);
    let size = chart_json.len() as u32;
    let mut out = Vec::with_capacity(30 + name.len() + chart_json.len() + 46 + name.len() + 22);

    // Local file header.
    out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
    out.extend_from_slice(&ZIP_DOS_TIME.to_le_bytes());
    out.extend_from_slice(&ZIP_DOS_DATE.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra field length
    out.extend_from_slice(name);
    out.extend_from_slice(chart_json);

    // Central directory header.
    let central_start = out.len() as u32;
    out.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes()); // version made by (MS-DOS)
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
    out.extend_from_slice(&ZIP_DOS_TIME.to_le_bytes());
    out.extend_from_slice(&ZIP_DOS_DATE.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra field length
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
    out.extend_from_slice(&(0o600u32 << 16).to_le_bytes()); // external attributes
    out.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    out.extend_from_slice(name);
    let central_size = out.len() as u32 - central_start;

    // End of central directory.
    out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // central directory disk
    out.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
    out.extend_from_slice(&1u16.to_le_bytes()); // total entries
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out
}
