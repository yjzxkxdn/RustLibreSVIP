//! Minimal stand-in for the `pypinyin` calls used by the Yuan77 plugin.
//!
//! Han characters are passed through unchanged (the pypinyin dictionary is not
//! part of this port); non-Chinese runs are preserved exactly like `pypinyin`,
//! which is byte-identical to Python for the ASCII golden fixtures.

use libresvip_core::core::lyric_phoneme::chinese::is_chinese_char;

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

pub fn lazy_pinyin(text: &str) -> Vec<String> {
    segments(text)
}

pub fn heteronym(text: &str) -> Vec<Vec<String>> {
    segments(text).into_iter().map(|item| vec![item]).collect()
}
