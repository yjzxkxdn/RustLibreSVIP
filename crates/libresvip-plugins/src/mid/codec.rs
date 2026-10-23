//! Charset helpers for the `mid` plugin.
//!
//! `midi_parser` decodes meta text with `bytes.decode(encoding, "ignore")` and
//! `midi_generator` encodes with `str.encode(encoding, "replace")`. Those two
//! error policies are reproduced on top of `encoding_rs`:
//!
//! * "ignore": malformed input is dropped (we decode with replacement and then
//!   drop the U+FFFD characters, which is equivalent for every input that does
//!   not legitimately decode to U+FFFD),
//! * "replace": unmappable characters become `?`, like Python.
//!
//! `ascii` is special-cased because the WHATWG label maps it to Windows-1252,
//! while Python's `ascii` codec rejects every non-ASCII byte.

use libresvip_core::error::{LibreSvipError, Result};

fn normalized(label: &str) -> String {
    label.trim().to_ascii_lowercase()
}

fn is_utf8(label: &str) -> bool {
    matches!(normalized(label).as_str(), "utf-8" | "utf8" | "u8")
}

fn is_ascii(label: &str) -> bool {
    matches!(normalized(label).as_str(), "ascii" | "us-ascii" | "646")
}

/// `bytes.decode(label, "ignore")`.
pub fn decode_ignore(bytes: &[u8], label: &str) -> Result<String> {
    if is_ascii(label) {
        return Ok(bytes
            .iter()
            .filter(|byte| **byte < 0x80)
            .map(|byte| *byte as char)
            .collect());
    }
    if is_utf8(label) {
        return Ok(String::from_utf8_lossy(bytes).replace('\u{FFFD}', ""));
    }
    let encoding = encoding_rs::Encoding::for_label(label.trim().as_bytes())
        .ok_or_else(|| unknown_encoding(label))?;
    let (decoded, _had_errors) = encoding.decode_without_bom_handling(bytes);
    Ok(decoded.replace('\u{FFFD}', ""))
}

/// `text.encode(label, "replace")`.
pub fn encode_replace(text: &str, label: &str) -> Result<Vec<u8>> {
    if is_utf8(label) {
        return Ok(text.as_bytes().to_vec());
    }
    if is_ascii(label) {
        return Ok(text
            .chars()
            .map(|character| {
                if (character as u32) < 0x80 {
                    character as u8
                } else {
                    b'?'
                }
            })
            .collect());
    }
    let encoding = encoding_rs::Encoding::for_label(label.trim().as_bytes())
        .ok_or_else(|| unknown_encoding(label))?;
    let mut encoder = encoding.new_encoder();
    let mut output: Vec<u8> = Vec::new();
    let mut rest = text;
    loop {
        let (result, read) =
            encoder.encode_from_utf8_to_vec_without_replacement(rest, &mut output, true);
        rest = &rest[read..];
        match result {
            encoding_rs::EncoderResult::InputEmpty => break,
            encoding_rs::EncoderResult::OutputFull => output.reserve(4096),
            encoding_rs::EncoderResult::Unmappable(_) => {
                output.push(b'?');
                let skip = rest
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
                if skip > rest.len() {
                    break;
                }
                rest = &rest[skip..];
            }
        }
    }
    Ok(output)
}

fn unknown_encoding(label: &str) -> LibreSvipError {
    LibreSvipError::msg(format!("unknown encoding: {label}"))
}
