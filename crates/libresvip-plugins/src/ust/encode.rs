//! Python codec names ↔ `encoding_rs` encodings.

use encoding_rs::Encoding;
use libresvip_core::{LibreSvipError, Result};

/// Resolve a Python codec name to an `encoding_rs` encoding.
///
/// `Encoding::for_label` covers the WHATWG labels (which is most of the names in
/// `supported_charset_names()`). Unknown names raise `LookupError` in Python, so
/// they fail the conversion here instead of silently producing UTF-8 mojibake.
/// (UTF-32 exists in Python but has no `encoding_rs` equivalent; it is treated
/// as unsupported — see README known differences.)
pub fn python_encoding(name: &str) -> Result<&'static Encoding> {
    let label = name.trim();
    if let Some(encoding) = Encoding::for_label(label.as_bytes()) {
        return Ok(encoding);
    }
    match label.to_ascii_lowercase().as_str() {
        "utf-16" | "utf16" => Ok(encoding_rs::UTF_16LE),
        _ => Err(LibreSvipError::msg(format!(
            "unknown text encoding: {name}"
        ))),
    }
}

/// `bytes.decode(name, errors="replace")`.
pub fn decode(bytes: &[u8], name: &str) -> Result<String> {
    let encoding = python_encoding(name)?;
    let (decoded, _, _) = encoding.decode(bytes);
    Ok(decoded.into_owned())
}

/// `str.encode(name)` — errors when a character cannot be represented, like
/// Python's strict encoder.
pub fn encode(text: &str, name: &str) -> Result<Vec<u8>> {
    let encoding = python_encoding(name)?;
    let (encoded, _, had_errors) = encoding.encode(text);
    if had_errors {
        return Err(LibreSvipError::msg(format!(
            "character cannot be encoded with {name}"
        )));
    }
    Ok(encoded.into_owned())
}
