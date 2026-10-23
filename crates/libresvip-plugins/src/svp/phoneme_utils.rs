//! Port of `libresvip.plugins.svp.phoneme_utils`.

use std::collections::HashMap;
use std::sync::OnceLock;

use libresvip_core::core::constants::DEFAULT_PHONEME;
use libresvip_core::core::lyric_phoneme::chinese::is_chinese_char;

use crate::svp::constants::{DEFAULT_DURATIONS, DEFAULT_PHONE_RATIO};

const PHONEME_CATEGORIES_JSON: &str = include_str!("phoneme_categories.json");
const PHONEME_DICTIONARY_JSON: &str = include_str!("phoneme_dictionary.json");

type LanguageMap = HashMap<String, String>;

struct PhonemeData {
    categories: HashMap<String, LanguageMap>,
    dictionary: HashMap<String, LanguageMap>,
    inverse: HashMap<String, LanguageMap>,
}

fn data() -> &'static PhonemeData {
    static DATA: OnceLock<PhonemeData> = OnceLock::new();
    DATA.get_or_init(|| {
        let categories: HashMap<String, LanguageMap> =
            serde_json::from_str(PHONEME_CATEGORIES_JSON).unwrap_or_default();
        let dictionary: HashMap<String, LanguageMap> =
            serde_json::from_str(PHONEME_DICTIONARY_JSON).unwrap_or_default();
        let mut inverse: HashMap<String, LanguageMap> = HashMap::new();
        for (language, mapping) in &dictionary {
            let target = inverse.entry(language.clone()).or_default();
            for (key, value) in mapping {
                target.entry(value.clone()).or_insert_with(|| key.clone());
            }
        }
        PhonemeData {
            categories,
            dictionary,
            inverse,
        }
    })
}

/// `sv_g2p_one`.
fn sv_g2p_one(buffer: &[String], language: Option<&str>) -> Vec<String> {
    let Some(language) = language else {
        return buffer.to_vec();
    };
    let (graphemes, default_phoneme): (Vec<String>, &str) = match language {
        "japanese" => (
            buffer
                .iter()
                .map(|part| {
                    libresvip_core::core::lyric_phoneme::japanese::to_romaji(part)
                        .unwrap_or_else(|| part.clone())
                })
                .collect(),
            "r a",
        ),
        "korean" => (buffer.to_vec(), "l 6"),
        "mandarin" => (get_pinyin_series(buffer, false), "l a"),
        "cantonese" => (
            buffer
                .iter()
                .map(|part| {
                    let stripped = strip_trailing_digits(part);
                    let stripped = stripped.trim();
                    if stripped.is_empty() {
                        part.clone()
                    } else {
                        stripped.to_owned()
                    }
                })
                .collect(),
            "l a",
        ),
        _ => return buffer.to_vec(),
    };
    let Some(dictionary) = data().dictionary.get(language) else {
        return graphemes
            .iter()
            .map(|_| default_phoneme.to_owned())
            .collect();
    };
    graphemes
        .into_iter()
        .map(|grapheme| {
            dictionary
                .get(&grapheme)
                .cloned()
                .unwrap_or_else(|| default_phoneme.to_owned())
        })
        .collect()
}

fn strip_trailing_digits(text: &str) -> String {
    let mut trimmed = text;
    while let Some(last) = trimmed.chars().last() {
        if last.is_ascii_digit() {
            trimmed = &trimmed[..trimmed.len() - last.len_utf8()];
        } else {
            break;
        }
    }
    trimmed.to_owned()
}

/// `sv_g2p`.
pub fn sv_g2p(lyrics: &[String], languages: &[String]) -> Vec<String> {
    let mut phoneme_list: Vec<String> = Vec::new();
    let mut builder: Vec<String> = Vec::new();
    let mut language: Option<String> = None;
    for (lyric, lyric_language) in lyrics.iter().zip(languages.iter()) {
        language = Some(lyric_language.clone());
        if lyric.chars().any(char::is_whitespace) && !lyric.chars().any(is_chinese_char) {
            if !builder.is_empty() {
                phoneme_list.extend(sv_g2p_one(&builder, language.as_deref()));
                builder.clear();
            }
            phoneme_list.push(lyric.clone());
        } else {
            builder.push(lyric.clone());
        }
    }
    if !builder.is_empty() {
        phoneme_list.extend(sv_g2p_one(&builder, language.as_deref()));
    }
    phoneme_list
}

/// `xsampa2pinyin`.
pub fn xsampa2pinyin(xsampa: &str, language: &str) -> String {
    let collapsed = collapse_whitespace(xsampa);
    data()
        .inverse
        .get(language)
        .and_then(|mapping| mapping.get(&collapsed))
        .cloned()
        .unwrap_or_else(|| DEFAULT_PHONEME.to_owned())
}

fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_whitespace = false;
    for character in text.trim().chars() {
        if character.is_whitespace() {
            if !in_whitespace {
                result.push(' ');
                in_whitespace = true;
            }
        } else {
            result.push(character);
            in_whitespace = false;
        }
    }
    result
}

/// `get_phoneme_categories` (`None` when the language is unknown or any part
/// has no category).
pub fn get_phoneme_categories(phoneme_parts: &[&str], language: &str) -> Option<Vec<String>> {
    let categories = data().categories.get(language)?;
    let mut result = Vec::with_capacity(phoneme_parts.len());
    for part in phoneme_parts {
        result.push(categories.get(*part)?.clone());
    }
    Some(result)
}

/// `number_of_phones`.
pub fn number_of_phones(phoneme: &str, language: &str) -> usize {
    let phoneme_parts: Vec<&str> = phoneme.split_whitespace().collect();
    match get_phoneme_categories(&phoneme_parts, language) {
        Some(categories) => categories.len(),
        None => 2,
    }
}

/// `default_phone_marks`.
pub fn default_phone_marks(phoneme: &str, language: &str) -> Vec<f64> {
    let phoneme_parts: Vec<&str> = phoneme.split_whitespace().collect();
    let res_len = phoneme_parts.len().max(2);
    let mut res = vec![0.0; res_len];
    if phoneme == "-" {
        return res;
    }
    if let Some(categories) = get_phoneme_categories(&phoneme_parts, language) {
        let mut index = 0usize;
        if matches!(categories[0].as_str(), "vowel" | "diphthong") {
            res[index] = DEFAULT_DURATIONS.get(&categories[0]).unwrap_or(0.0);
            index += 1;
        }
        if res_len > index {
            for slot in &mut res[index..res_len] {
                *slot = DEFAULT_PHONE_RATIO;
            }
        }
    }
    res
}

/// Approximate `get_pinyin_series` from
/// `libresvip.core.lyric_phoneme.chinese`. Hanzi runs cannot be romanised
/// without `pypinyin` data, so each character is kept as its own grapheme and
/// falls back to the default phoneme during dictionary lookup.
fn get_pinyin_series(parts: &[String], filter_non_chinese: bool) -> Vec<String> {
    let mut result_items: Vec<String> = Vec::new();
    let mut item_counts = vec![0usize; parts.len()];
    let mut chinese = String::new();
    for (index, part) in parts.iter().enumerate() {
        let mut count = 0usize;
        let mut is_letter = false;
        let mut non_chinese = String::new();
        for character in part.chars() {
            if character.is_whitespace() || character == '-' {
                if !non_chinese.is_empty() {
                    result_items.push(std::mem::take(&mut non_chinese));
                    count += 1;
                }
            } else if is_chinese_char(character) {
                if !non_chinese.is_empty() {
                    result_items.push(std::mem::take(&mut non_chinese));
                    count += 1;
                }
                chinese.push(character);
                count += 1;
            } else {
                if !chinese.is_empty() {
                    flush_chinese(&mut chinese, &mut result_items);
                }
                if character.is_ascii_alphabetic() {
                    if !is_letter && !non_chinese.is_empty() {
                        result_items.push(std::mem::take(&mut non_chinese));
                        count += 1;
                    }
                    non_chinese.push(character);
                    is_letter = true;
                } else if !filter_non_chinese {
                    if !non_chinese.is_empty() && is_letter {
                        result_items.push(std::mem::take(&mut non_chinese));
                        count += 1;
                    }
                    non_chinese.push(character);
                    is_letter = false;
                }
            }
        }
        if !non_chinese.is_empty() {
            result_items.push(non_chinese);
            count += 1;
        }
        item_counts[index] = count;
    }
    if !chinese.is_empty() {
        flush_chinese(&mut chinese, &mut result_items);
    }
    let mut pinyin_list = vec![String::new(); parts.len()];
    let mut index = 0usize;
    for (part_index, count) in item_counts.iter().copied().enumerate() {
        if count == 0 {
            continue;
        }
        let mut pinyin = result_items[index].clone();
        index += 1;
        for _ in 1..count {
            if index >= result_items.len() {
                break;
            }
            let previous_alnum = result_items[index - 1]
                .chars()
                .last()
                .map(char::is_alphanumeric)
                .unwrap_or(false);
            let current_alnum = result_items[index]
                .chars()
                .next()
                .map(char::is_alphanumeric)
                .unwrap_or(false);
            if previous_alnum && current_alnum {
                pinyin.push(' ');
            }
            pinyin.push_str(&result_items[index]);
            index += 1;
        }
        pinyin_list[part_index] = pinyin;
    }
    pinyin_list
}

fn flush_chinese(chinese: &mut String, result_items: &mut Vec<String>) {
    for character in chinese.chars() {
        result_items.push(character.to_string());
    }
    chinese.clear();
}
