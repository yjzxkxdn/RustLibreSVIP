//! Port of `libresvip.utils.text` (the parts used by the CLI and plugins).

use std::fmt::Write as _;

/// Decodes arbitrary bytes, guessing the encoding like `charset_normalizer`.
///
/// BOMs win, then UTF-8, then the charset most commonly seen in the wild for
/// this family of formats (GB18030 for Chinese, Shift_JIS for Japanese).
pub fn to_unicode(content: &[u8]) -> String {
    if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&content[3..]).into_owned();
    }
    if content.starts_with(&[0xFF, 0xFE]) {
        let (decoded, _, _) = encoding_rs::UTF_16LE.decode(content);
        return decoded.into_owned();
    }
    if content.starts_with(&[0xFE, 0xFF]) {
        let (decoded, _, _) = encoding_rs::UTF_16BE.decode(content);
        return decoded.into_owned();
    }
    if let Ok(text) = std::str::from_utf8(content) {
        return text.to_owned();
    }
    let (decoded, _, had_errors) = encoding_rs::GB18030.decode(content);
    if !had_errors {
        return decoded.into_owned();
    }
    let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(content);
    decoded.into_owned()
}

/// `shorten_error_message` — wraps at 70 columns and elides the middle after
/// 30 lines.
pub fn shorten_error_message(message: Option<&str>) -> String {
    let Some(message) = message else {
        return String::new();
    };
    let lines = wrap(message, 70);
    if lines.len() > 30 {
        let mut result: Vec<String> = lines[..15].to_vec();
        result.push("...".to_owned());
        result.extend_from_slice(&lines[lines.len() - 15..]);
        result.join("\n")
    } else {
        lines.join("\n")
    }
}

/// Minimal `textwrap.wrap` replacement (breaks on whitespace, then hard-wraps).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            if word.chars().count() > width {
                let mut chunk = String::new();
                for character in word.chars() {
                    chunk.push(character);
                    if chunk.chars().count() == width {
                        lines.push(std::mem::take(&mut chunk));
                    }
                }
                current = chunk;
            } else {
                current.push_str(word);
            }
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// `uuid_str()` — a random v4 UUID.
pub fn uuid_str() -> String {
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};

    thread_local! {
        static STATE: Cell<u64> = Cell::new(0);
    }
    let mut seed = STATE.with(|state| {
        let mut value = state.get();
        if value == 0 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15);
            value = nanos ^ (std::process::id() as u64).wrapping_mul(0x2545_F491_4F6C_DD1D);
            if value == 0 {
                value = 0x9E37_79B9_7F4A_7C15;
            }
        }
        value
    });
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(8) {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        for (index, byte) in chunk.iter_mut().enumerate() {
            *byte = ((seed >> (index * 8)) & 0xFF) as u8;
        }
    }
    STATE.with(|state| state.set(seed));
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let mut out = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// `supported_charset_names()` — the encodings offered by the CLI prompts.
///
/// Generated from Python's `encodings.aliases` + `charset_normalizer`; kept in
/// `charset_list.rs` so it can be regenerated verbatim.
pub fn supported_charset_names() -> &'static [&'static str] {
    crate::utils::text::charset_list::CHARSET_NAMES
}

pub mod charset_list;
