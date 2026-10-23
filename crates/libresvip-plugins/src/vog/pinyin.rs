//! Minimal stand-in for `pypinyin.lazy_pinyin` (see the `nn` plugin's note on
//! the missing dictionary). Non-Chinese runs are preserved like pypinyin.

use libresvip_core::core::lyric_phoneme::chinese::is_chinese_char;

pub fn lazy_pinyin(text: &str) -> Vec<String> {
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
