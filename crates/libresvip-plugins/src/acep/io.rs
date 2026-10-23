//! Port of `libresvip.plugins.acep.acep_io`.
//!
//! `.acep` files come in two flavours:
//!
//! * the JSON container (`AcepFile`) used by ACE Studio ≤ 2.0.6, where the
//!   actual project is zstd-compressed and base64-encoded in `content`,
//! * the `ACEP2` binary container (ACE Studio ≥ 2.0.7) holding a CBOR payload.
//!
//! Encrypted legacy containers (`version` 1 and 2) need AES-CBC, which the
//! Rust port does not implement: they are reported as unsupported instead.

use std::io::{Cursor, Read, Write};

use base64::Engine;
use ciborium::value::Value as CborValue;
use serde_json::{Map, Value};
use zip::ZipArchive;

use libresvip_core::error::{LibreSvipError, Result};

use super::model::AcepProject;
use super::options::AcepSerialization;

pub const ACEP2_MAGIC: &[u8; 5] = b"ACEP2";
pub const ACEP2_FLAG: u8 = 0x01;

const BASE64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::STANDARD;

/// `libresvip.plugins.acep.acep_io.AcepFile` (the JSON container).
#[derive(Clone, Debug, serde::Deserialize)]
struct AcepFile {
    #[serde(rename = "compressMethod", default = "default_compress_method")]
    compress_method: String,
    #[serde(rename = "debugInfo", default)]
    debug_info: AcepDebug,
    #[serde(default)]
    salt: String,
    #[serde(default = "default_version")]
    version: i64,
    #[serde(default)]
    content: String,
}

fn default_compress_method() -> String {
    "zstd".to_owned()
}

fn default_version() -> i64 {
    1000
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct AcepDebug {
    #[serde(default = "default_os")]
    os: String,
    #[serde(default = "default_platform")]
    platform: String,
    #[serde(default = "default_debug_version")]
    version: String,
}

fn default_os() -> String {
    "windows".to_owned()
}

fn default_platform() -> String {
    "pc".to_owned()
}

fn default_debug_version() -> String {
    "10".to_owned()
}

impl Default for AcepDebug {
    fn default() -> Self {
        Self {
            os: default_os(),
            platform: default_platform(),
            version: default_debug_version(),
        }
    }
}

fn decode_base64(text: &str) -> Result<Vec<u8>> {
    let trimmed = text.trim();
    if let Ok(bytes) = BASE64.decode(trimmed) {
        return Ok(bytes);
    }
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(trimmed)
        .map_err(|error| LibreSvipError::msg(format!("invalid base64 content: {error}")))
}

/// `Acep2Header` — 51 fixed bytes plus the prefixed metadata and padding.
struct Acep2Header {
    content_offset: usize,
    compressed_content_size: usize,
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

impl Acep2Header {
    fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 34 || &bytes[0..5] != ACEP2_MAGIC {
            return Err(LibreSvipError::msg("invalid ACEP2 header"));
        }
        if bytes[5] != ACEP2_FLAG {
            return Err(LibreSvipError::msg("invalid ACEP2 flags"));
        }
        let _header_size =
            read_u16(bytes, 6).ok_or_else(|| LibreSvipError::msg("truncated ACEP2 header"))?;
        let content_offset = read_u64(bytes, 8)
            .ok_or_else(|| LibreSvipError::msg("truncated ACEP2 header"))?
            as usize;
        let compressed_content_size = read_u64(bytes, 16)
            .ok_or_else(|| LibreSvipError::msg("truncated ACEP2 header"))?
            as usize;
        let metadata_length = read_u16(bytes, 32)
            .ok_or_else(|| LibreSvipError::msg("truncated ACEP2 header"))?
            as usize;
        let metadata_end = 34 + metadata_length;
        let metadata_flag = *bytes
            .get(metadata_end)
            .ok_or_else(|| LibreSvipError::msg("truncated ACEP2 header"))?;
        if metadata_flag != ACEP2_FLAG {
            return Err(LibreSvipError::msg("invalid ACEP2 metadata flag"));
        }
        if bytes.len() < metadata_end + 17 {
            return Err(LibreSvipError::msg("truncated ACEP2 header"));
        }
        if content_offset < 51 + metadata_length || content_offset > bytes.len() {
            return Err(LibreSvipError::msg("invalid ACEP2 content offset"));
        }
        if content_offset + compressed_content_size > bytes.len() {
            return Err(LibreSvipError::msg("truncated ACEP2 content"));
        }
        Ok(Self {
            content_offset,
            compressed_content_size,
        })
    }

    fn build(
        content_offset: usize,
        compressed_content_size: usize,
        content_size: usize,
        encrypted_metadata: &[u8],
    ) -> Result<Vec<u8>> {
        if content_offset < 51 + encrypted_metadata.len() {
            return Err(LibreSvipError::msg("invalid ACEP2 content offset"));
        }
        let mut bytes = Vec::with_capacity(content_offset);
        bytes.extend_from_slice(ACEP2_MAGIC);
        bytes.push(ACEP2_FLAG);
        bytes.extend_from_slice(&0u16.to_le_bytes()); // header_size
        bytes.extend_from_slice(&(content_offset as u64).to_le_bytes());
        bytes.extend_from_slice(&(compressed_content_size as u64).to_le_bytes());
        bytes.extend_from_slice(&(content_size as u64).to_le_bytes());
        bytes.extend_from_slice(&(encrypted_metadata.len() as u16).to_le_bytes());
        bytes.extend_from_slice(encrypted_metadata);
        bytes.push(ACEP2_FLAG);
        bytes.extend_from_slice(&[0u8; 16]); // content_hash
        bytes.resize(content_offset, 0); // padding
        Ok(bytes)
    }
}

fn zstd_compress(data: &[u8]) -> Result<Vec<u8>> {
    zstd::bulk::compress(data, zstd::DEFAULT_COMPRESSION_LEVEL)
        .map_err(|error| LibreSvipError::msg(format!("zstd compression failed: {error}")))
}

fn zstd_decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(data))
        .map_err(|error| LibreSvipError::msg(format!("zstd decompression failed: {error}")))?;
    let mut output = Vec::new();
    decoder
        .read_to_end(&mut output)
        .map_err(|error| LibreSvipError::msg(format!("zstd decompression failed: {error}")))?;
    Ok(output)
}

fn cbor_to_json(value: CborValue) -> Result<Value> {
    Ok(match value {
        CborValue::Null => Value::Null,
        CborValue::Bool(value) => Value::Bool(value),
        CborValue::Integer(value) => {
            let value = i128::from(value);
            match i64::try_from(value) {
                Ok(value) => Value::Number(value.into()),
                Err(_) => Value::Number((value as u64).into()),
            }
        }
        CborValue::Float(value) => serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        CborValue::Text(value) => Value::String(value),
        CborValue::Bytes(value) => Value::String(BASE64.encode(value)),
        CborValue::Array(values) => Value::Array(
            values
                .into_iter()
                .map(cbor_to_json)
                .collect::<Result<Vec<Value>>>()?,
        ),
        CborValue::Map(entries) => {
            let mut map = Map::new();
            for (key, value) in entries {
                let key = match key {
                    CborValue::Text(key) => key,
                    CborValue::Integer(key) => i128::from(key).to_string(),
                    other => {
                        return Err(LibreSvipError::msg(format!(
                            "unsupported CBOR map key: {other:?}"
                        )))
                    }
                };
                map.insert(key, cbor_to_json(value)?);
            }
            Value::Object(map)
        }
        CborValue::Tag(_, inner) => cbor_to_json(*inner)?,
        _ => Value::Null,
    })
}

/// `decompress_ace_studio_project`.
pub fn decompress_project(bytes: &[u8]) -> Result<Value> {
    if bytes.len() >= 5 && &bytes[0..5] == ACEP2_MAGIC {
        let header = Acep2Header::parse(bytes)?;
        let compressed = &bytes
            [header.content_offset..header.content_offset + header.compressed_content_size];
        let decompressed = zstd_decompress(compressed)?;
        let value: CborValue = ciborium::de::from_reader(Cursor::new(decompressed))
            .map_err(|error| LibreSvipError::Parse(format!("invalid CBOR content: {error}")))?;
        return cbor_to_json(value);
    }
    let text = String::from_utf8_lossy(bytes);
    let acep_file: AcepFile = serde_json::from_str(&text)?;
    let _ = (&acep_file.compress_method, &acep_file.debug_info, &acep_file.salt);
    if acep_file.version == 1 || acep_file.version == 2 {
        return Err(LibreSvipError::UnsupportedProjectVersion(
            acep_file.version.to_string(),
        ));
    }
    let content = decode_base64(&acep_file.content)?;
    let decompressed = zstd_decompress(&content)?;
    serde_json::from_slice(&decompressed)
        .map_err(|error| LibreSvipError::Parse(format!("invalid project JSON: {error}")))
}

/// Reads the first top-level `.acep` entry of an `.acet` (zip) archive.
pub fn read_acet_entry(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    let cursor = Cursor::new(bytes.to_vec());
    let mut archive = ZipArchive::new(cursor)
        .map_err(|error| LibreSvipError::msg(format!("invalid .acet archive: {error}")))?;
    for index in 0..archive.len() {
        let (name, is_dir) = {
            let file = archive
                .by_index(index)
                .map_err(|error| LibreSvipError::msg(format!("invalid .acet archive: {error}")))?;
            (file.name().to_owned(), file.is_dir())
        };
        if is_dir {
            continue;
        }
        // `zipfile.Path.iterdir()` yields top-level entries; `item.name` is the
        // final path component.
        if name.contains('/') || name.contains('\\') {
            continue;
        }
        if name.to_lowercase().ends_with(".acep") {
            let mut file = archive
                .by_index(index)
                .map_err(|error| LibreSvipError::msg(format!("invalid .acet archive: {error}")))?;
            let mut content = Vec::new();
            file.read_to_end(&mut content)
                .map_err(|error| LibreSvipError::msg(format!("invalid .acet entry: {error}")))?;
            return Ok(Some(content));
        }
    }
    Ok(None)
}

/// A JSON formatter that matches `json.dumps` with `ensure_ascii=True` and
/// compact separators.
pub struct PythonJsonFormatter;

fn write_escaped_char<W: ?Sized + Write>(writer: &mut W, character: char) -> std::io::Result<()> {
    const HEX: [u8; 16] = *b"0123456789abcdef";
    let mut units: [u16; 2] = [0; 2];
    let count = character.encode_utf16(&mut units).len();
    for unit in units.iter().take(count) {
        writer.write_all(b"\\u")?;
        writer.write_all(&[
            HEX[(*unit >> 12) as usize & 0x0f],
            HEX[(*unit >> 8) as usize & 0x0f],
            HEX[(*unit >> 4) as usize & 0x0f],
            HEX[*unit as usize & 0x0f],
        ])?;
    }
    Ok(())
}

impl serde_json::ser::Formatter for PythonJsonFormatter {
    fn write_string_fragment<W>(&mut self, writer: &mut W, fragment: &str) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        let mut start = 0usize;
        for (index, character) in fragment.char_indices() {
            let code = character as u32;
            if (0x20..=0x7e).contains(&code) && character != '"' && character != '\\' {
                continue;
            }
            if start < index {
                writer.write_all(fragment[start..index].as_bytes())?;
            }
            match character {
                '"' => writer.write_all(b"\\\"")?,
                '\\' => writer.write_all(b"\\\\")?,
                '\n' => writer.write_all(b"\\n")?,
                '\r' => writer.write_all(b"\\r")?,
                '\t' => writer.write_all(b"\\t")?,
                '\u{08}' => writer.write_all(b"\\b")?,
                '\u{0c}' => writer.write_all(b"\\f")?,
                _ => write_escaped_char(writer, character)?,
            }
            start = index + character.len_utf8();
        }
        if start < fragment.len() {
            writer.write_all(fragment[start..].as_bytes())?;
        }
        Ok(())
    }
}

/// `json.dumps(value)` with python's compact separators and ASCII escaping.
pub fn to_python_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut serializer =
        serde_json::Serializer::with_formatter(Vec::new(), PythonJsonFormatter);
    value
        .serialize(&mut serializer)
        .map_err(|error| LibreSvipError::Parse(format!("JSON serialization failed: {error}")))?;
    Ok(serializer.into_inner())
}

/// `compress_ace_studio_project`.
pub fn compress_project(
    ace_project: &AcepProject,
    serialization: AcepSerialization,
) -> Result<Vec<u8>> {
    match serialization {
        AcepSerialization::Json => {
            let raw_content = to_python_json(ace_project)?;
            let compressed = zstd_compress(&raw_content)?;
            let mut outer = Map::new();
            outer.insert(
                "compressMethod".to_owned(),
                Value::String("zstd".to_owned()),
            );
            outer.insert(
                "debugInfo".to_owned(),
                serde_json::to_value(AcepDebug::default())?,
            );
            outer.insert("salt".to_owned(), Value::String(String::new()));
            outer.insert("version".to_owned(), Value::Number(1000.into()));
            outer.insert(
                "content".to_owned(),
                Value::String(BASE64.encode(&compressed)),
            );
            to_python_json(&Value::Object(outer))
        }
        AcepSerialization::Cbor => {
            let mut raw_content = Vec::new();
            ciborium::ser::into_writer(ace_project, &mut raw_content)
                .map_err(|error| LibreSvipError::Parse(format!("CBOR serialization failed: {error}")))?;
            let content_size = raw_content.len();
            let compressed = zstd_compress(&raw_content)?;
            let compressed_content_size = compressed.len();
            let mut content = Acep2Header::build(
                192,
                compressed_content_size,
                content_size,
                &vec![0u8; 138],
            )?;
            content.extend_from_slice(&compressed);
            Ok(content)
        }
    }
}
