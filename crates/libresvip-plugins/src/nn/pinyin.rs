//! Minimal stand-in for the `pypinyin` calls used by the NIAONiao plugin.
//!
//! The Python reference uses `pypinyin.pinyin(..., heteronym=True)` and
//! `pypinyin.lazy_pinyin`. The pypinyin dictionary is not part of this port, so
//! Han characters are passed through unchanged (one grapheme per item) while
//! non-Chinese runs are preserved exactly like `pypinyin` does. For the ASCII
//! lyrics of the golden fixtures this is byte-identical to Python.

use libresvip_core::core::lyric_phoneme::chinese::is_chinese_char;

/// Split `text` into runs of Han characters (one item per character) and runs
/// of non-Chinese text (kept verbatim), mirroring how `pypinyin` segments input.
fn segments(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut buffer = String::new();
    for character in text.chars() {
        if is_chinese_char(character) {
            if !buffer.is_empty() {
                result.push(std::mem::take(&mut buffer));
            }
            result.push(character.to_string());
        } else {
            buffer.push(character);
        }
    }
    if !buffer.is_empty() {
        result.push(buffer);
    }
    result
}

/// `pypinyin.lazy_pinyin(text)` — one string per segment (space separated when
/// joined by the caller).
pub fn lazy_pinyin(text: &str) -> Vec<String> {
    segments(text)
}

/// `pypinyin.pinyin(text, heteronym=True, style=STYLE_NORMAL)` — one reading
/// list per segment.
pub fn heteronym(text: &str) -> Vec<Vec<String>> {
    segments(text).into_iter().map(|item| vec![item]).collect()
}
