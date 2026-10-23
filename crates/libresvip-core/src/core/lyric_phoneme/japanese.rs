//! Port of `libresvip.core.lyric_phoneme.japanese`.
//!
//! `to_romaji` reproduces `wanakana.to_romaji` — the kana→romaji trie, the
//! katakana→hiragana pass that resolves long vowels, and `apply_mapping`'s
//! longest-prefix walk — with LibreSVIP's custom `{"っ": "cl"}` mapping merged
//! into the tree. The trie lives in `assets/phoneme/kana_romaji.json`
//! (see `scripts/gen_phoneme_data.py`).
//!
//! Kana script conversions (`is_kana`, `is_romaji`, `to_hiragana`,
//! `to_katakana`) remain the simplified code-point shifts they were before; only
//! `to_romaji` is exact.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::core::lyric_phoneme::chinese::is_chinese_char;

/// `assets/phoneme/kana_romaji.json` — wanakana's Hepburn kana→romaji trie with
/// the custom `っ` mapping merged in.
const KANA_ROMAJI_JSON: &str = include_str!("../../../../../assets/phoneme/kana_romaji.json");

/// `legato_chars` from `vocaloid_xsampa`.
pub const LEGATO_CHARS: [&str; 3] = ["ー", "−", "-"];

/// `wanakana.to_romaji` with `custom_romaji_mapping={"っ": "cl"}`.
///
/// `None` mirrors the inputs on which the Python implementation raises (a long
/// vowel mark after a kana whose romaji does not end in a vowel, e.g. `ンー`).
pub fn to_romaji(text: &str) -> Option<String> {
    if text.is_empty() {
        return Some(String::new());
    }
    let tree = kana_tree()?;
    let hiragana = katakana_to_hiragana_for_romaji(text, tree)?;
    let characters: Vec<char> = hiragana.chars().collect();
    Some(tree.translate(&characters))
}

/// `wanakana.is_kana`.
pub fn is_kana(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|character| {
            let code = character as u32;
            (0x3040..=0x309F).contains(&code) || (0x30A0..=0x30FF).contains(&code)
        })
}

/// `wanakana.is_romaji` (approximation of the library check).
pub fn is_romaji(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphabetic() || " -'.".contains(character))
}

/// `wanakana.to_hiragana`: katakana → hiragana.
pub fn to_hiragana(text: &str) -> String {
    text.chars()
        .map(|character| {
            let code = character as u32;
            if (0x30A1..=0x30F6).contains(&code) {
                char::from_u32(code - 0x60).unwrap_or(character)
            } else {
                character
            }
        })
        .collect()
}

/// `wanakana.to_katakana`: hiragana → katakana.
pub fn to_katakana(text: &str) -> String {
    text.chars()
        .map(|character| {
            let code = character as u32;
            if (0x3041..=0x3096).contains(&code) {
                char::from_u32(code + 0x60).unwrap_or(character)
            } else {
                character
            }
        })
        .collect()
}

/// Characters that keep the previous syllable going (legato).
pub fn is_legato_char(text: &str) -> bool {
    LEGATO_CHARS.contains(&text)
}

/// Keep Chinese characters untouched in kana conversions.
pub fn contains_chinese(text: &str) -> bool {
    text.chars().any(is_chinese_char)
}

const PROLONGED_SOUND_MARK: char = '\u{30FC}';
const KANA_SLASH_DOT: char = '\u{30FB}';

fn is_char_long_dash(character: char) -> bool {
    character == PROLONGED_SOUND_MARK
}

fn is_char_slash_dot(character: char) -> bool {
    character == KANA_SLASH_DOT
}

/// `wanakana.utils.japanese.katakana.is_char_katakana` — note that this includes
/// the prolonged sound mark and every code point up to it.
fn is_char_katakana(character: char) -> bool {
    is_char_long_dash(character) || (0x30A1..=0x30FC).contains(&(character as u32))
}

/// `is_kana_as_symbol`: the small `ヶ` / `ヵ`.
fn is_kana_as_symbol(character: char) -> bool {
    matches!(character, 'ヶ' | 'ヵ')
}

/// `LONG_VOWELS` from `wanakana.utils.japanese.katakana_to_hiragana`.
fn long_vowel(character: char) -> Option<char> {
    match character {
        'a' => Some('あ'),
        'i' => Some('い'),
        'u' => Some('う'),
        'e' => Some('え'),
        'o' => Some('う'),
        _ => None,
    }
}

fn shift_to_hiragana(character: char) -> Option<char> {
    char::from_u32((character as u32).checked_sub(0x60)?)
}

/// `wanakana.utils.japanese.katakana_to_hiragana.katakana_to_hiragana(input,
/// to_romaji, True)`.
///
/// The `to_romaji` used to look back at the previous kana is wanakana's own
/// implementation (without LibreSVIP's custom mapping); the merged tree differs
/// only for `っ`, where both variants fail, so the merged tree is reused.
fn katakana_to_hiragana_for_romaji(input: &str, tree: &KanaTree) -> Option<String> {
    let characters: Vec<char> = input.chars().collect();
    let mut previous_kana: Option<char> = None;
    let mut hiragana = String::new();
    for (index, &character) in characters.iter().enumerate() {
        if is_char_slash_dot(character)
            || (index == 0 && is_char_long_dash(character))
            || is_kana_as_symbol(character)
        {
            hiragana.push(character);
        } else if previous_kana.is_some() && index > 0 && is_char_long_dash(character) {
            // Transform long vowels: 'オー' becomes 'おお', 'コー' becomes 'こお'.
            let romaji = tree.translate(&[previous_kana?]);
            let last = romaji.chars().last()?;
            if is_char_katakana(characters[index - 1]) && last == 'o' {
                hiragana.push('o');
            } else {
                hiragana.push(long_vowel(last)?);
            }
        } else if !is_char_long_dash(character) && is_char_katakana(character) {
            let shifted = shift_to_hiragana(character)?;
            previous_kana = Some(shifted);
            hiragana.push(shifted);
        } else {
            previous_kana = None;
            hiragana.push(character);
        }
    }
    Some(hiragana)
}

/// One node of wanakana's translation trie.
#[derive(Debug)]
struct KanaNode {
    /// The `""` key: the value produced when the walk stops here. `None` only for
    /// the root, which `apply_mapping` never parses directly.
    value: Option<String>,
    children: BTreeMap<char, usize>,
    /// Child keys longer than one character. `apply_mapping` only ever indexes a
    /// node by a single character, so these are unreachable — but `len(keys)` is
    /// part of the algorithm, so they are counted.
    extra_keys: usize,
}

impl KanaNode {
    /// `len(tree.keys())` in the Python implementation.
    fn key_count(&self) -> usize {
        self.children.len() + self.extra_keys + usize::from(self.value.is_some())
    }
}

#[derive(Debug)]
struct KanaTree {
    nodes: Vec<KanaNode>,
    root: usize,
}

impl KanaTree {
    /// `wanakana.utils.kana_mapping.apply_mapping(input, map, convert_ending=True)`,
    /// reduced to the concatenation of the produced tokens.
    fn translate(&self, input: &[char]) -> String {
        let mut output = String::new();
        let mut offset = 0;
        while offset < input.len() {
            match self.nodes[self.root].children.get(&input[offset]) {
                Some(&node) => offset += self.walk(node, &input[offset..], &mut output),
                None => {
                    // Unknown character: it is echoed back unchanged.
                    output.push(input[offset]);
                    offset += 1;
                }
            }
        }
        output
    }

    /// `apply_mapping`'s `parse`/`new_chunk` pair; returns the number of consumed
    /// characters (always at least one, so the caller always makes progress).
    fn walk(&self, mut node: usize, rest: &[char], output: &mut String) -> usize {
        let mut consumed = 1;
        loop {
            let current = &self.nodes[node];
            if consumed == rest.len() {
                if let Some(value) = &current.value {
                    output.push_str(value);
                }
                return consumed;
            }
            if current.key_count() == 1 {
                if let Some(value) = &current.value {
                    output.push_str(value);
                }
                return consumed;
            }
            match current.children.get(&rest[consumed]) {
                Some(&child) => {
                    node = child;
                    consumed += 1;
                }
                None => {
                    if let Some(value) = &current.value {
                        output.push_str(value);
                    }
                    return consumed;
                }
            }
        }
    }
}

fn kana_tree() -> Option<&'static KanaTree> {
    static KANA_TREE: OnceLock<Option<KanaTree>> = OnceLock::new();
    KANA_TREE.get_or_init(|| build_kana_tree().ok()).as_ref()
}

fn build_kana_tree() -> Result<KanaTree, String> {
    let value: serde_json::Value =
        serde_json::from_str(KANA_ROMAJI_JSON).map_err(|error| error.to_string())?;
    let mut nodes = Vec::new();
    let root = build_kana_node(&value, &mut nodes)
        .ok_or_else(|| "the kana→romaji trie root is not an object".to_string())?;
    Ok(KanaTree { nodes, root })
}

fn build_kana_node(value: &serde_json::Value, nodes: &mut Vec<KanaNode>) -> Option<usize> {
    let object = value.as_object()?;
    let mut node = KanaNode {
        value: None,
        children: BTreeMap::new(),
        extra_keys: 0,
    };
    let mut children = Vec::new();
    for (key, child) in object {
        if key.is_empty() {
            node.value = child.as_str().map(str::to_string);
            continue;
        }
        let mut characters = key.chars();
        match (characters.next(), characters.next()) {
            (Some(character), None) => children.push((character, child.clone())),
            _ => node.extra_keys += 1,
        }
    }
    let index = nodes.len();
    nodes.push(node);
    for (character, child) in children {
        if let Some(child_index) = build_kana_node(&child, nodes) {
            nodes[index].children.insert(character, child_index);
        }
    }
    Some(index)
}
