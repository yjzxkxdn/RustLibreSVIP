//! Port of `libresvip.core.lyric_phoneme.chinese`.
//!
//! The Python implementation delegates Hanzi → Pinyin to `pypinyin`
//! (`lazy_pinyin` in `Style.NORMAL` with `errors=lambda x: " "`). The character
//! and phrase dictionaries, already reduced to their tone-less first reading,
//! live in `assets/phoneme/` (see `scripts/gen_phoneme_data.py`) and are parsed
//! lazily on first use.
//!
//! The segmentation reproduces both stages of `pypinyin`:
//!
//! 1. `pypinyin.seg.simpleseg` splits a string into runs of Han and non-Han
//!    characters (`RE_HANS`),
//! 2. every Han run is cut by `pypinyin.seg.mmseg` (forward maximum matching over
//!    the phrase dictionary, `no_non_phrases=True`), which is what makes
//!    polyphonic characters read differently inside words.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::error::{LibreSvipError, Result};

/// `assets/phoneme/pinyin_normal.tsv` — `<char>\t<pinyin>` for every entry of
/// pypinyin's `pinyin_dict` (first reading, no tone). Embedded as a zlib
/// stream (~40% of the raw size); regenerate the `.zlib` files with
/// `python scripts/compress_phoneme_assets.py`.
const PINYIN_NORMAL_TSV_ZLIB: &[u8] =
    include_bytes!("../../../../../assets/phoneme/pinyin_normal.tsv.zlib");
/// `assets/phoneme/phrases.tsv` — `<phrase>\t<syllable> <syllable> ...`.
const PHRASES_TSV_ZLIB: &[u8] = include_bytes!("../../../../../assets/phoneme/phrases.tsv.zlib");

/// The decompressed `<char>\t<pinyin>` table, or `None` on inflate failure.
static PINYIN_NORMAL_TSV: OnceLock<Option<String>> = OnceLock::new();
/// The decompressed `<phrase>\t<syllables>` table, or `None` on failure.
static PHRASES_TSV: OnceLock<Option<String>> = OnceLock::new();

fn pinyin_normal_tsv() -> Option<&'static str> {
    PINYIN_NORMAL_TSV
        .get_or_init(|| {
            crate::compression::zlib_decompress(PINYIN_NORMAL_TSV_ZLIB)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
        })
        .as_deref()
}

fn phrases_tsv() -> Option<&'static str> {
    PHRASES_TSV
        .get_or_init(|| {
            crate::compression::zlib_decompress(PHRASES_TSV_ZLIB)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
        })
        .as_deref()
}

/// `zhon.hanzi.characters` — the characters matching Python's `CHINESE_RE`.
pub fn is_chinese_char(character: char) -> bool {
    matches!(
        character as u32,
        0x3007
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2F800..=0x2FA1F
    )
}

/// `pypinyin.constants.RE_HANS` — note that this range set is *not* identical to
/// `zhon.hanzi.characters`; pypinyin decides on its own what it can romanise.
fn is_hans_char(character: char) -> bool {
    matches!(
        character as u32,
        0x3007
            | 0xE815..=0xE864
            | 0xFA18
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A703..=0x2B73F
            | 0x2B740..=0x2B81D
            | 0x2B825..=0x2BF6E
            | 0x2C029..=0x2CE93
            | 0x2D016
            | 0x2D11B..=0x2EBD9
            | 0x2F80A..=0x2FA1F
            | 0x300F7..=0x31288
    )
}

pub fn contains_chinese(text: &str) -> bool {
    text.chars().any(is_chinese_char)
}

/// `re`'s `\s`: Unicode `White_Space` plus the information separators `\x1c`-`\x1f`
/// that CPython's `Py_UNICODE_ISSPACE` also accepts.
fn is_python_space(character: char) -> bool {
    character.is_whitespace() || matches!(character as u32, 0x1C..=0x1F)
}

static PINYIN_DICT: OnceLock<Result<HashMap<char, String>>> = OnceLock::new();
static PHRASE_DATA: OnceLock<Result<PhraseData>> = OnceLock::new();

fn pinyin_dict() -> Result<&'static HashMap<char, String>> {
    match PINYIN_DICT.get_or_init(load_pinyin_dict) {
        Ok(dictionary) => Ok(dictionary),
        Err(error) => Err(LibreSvipError::msg(format!(
            "pinyin dictionary is unavailable: {error}"
        ))),
    }
}

fn phrase_data() -> Result<&'static PhraseData> {
    match PHRASE_DATA.get_or_init(load_phrase_data) {
        Ok(data) => Ok(data),
        Err(error) => Err(LibreSvipError::msg(format!(
            "pinyin phrase dictionary is unavailable: {error}"
        ))),
    }
}

fn load_pinyin_dict() -> Result<HashMap<char, String>> {
    let tsv = pinyin_normal_tsv()
        .ok_or_else(|| LibreSvipError::msg("pinyin dictionary asset failed to decompress"))?;
    let mut dictionary = HashMap::with_capacity(43_000);
    for line in tsv.lines() {
        let Some((character, pinyin)) = line.split_once('\t') else {
            continue;
        };
        let mut chars = character.chars();
        let (Some(character), None) = (chars.next(), chars.next()) else {
            continue;
        };
        dictionary.insert(character, pinyin.to_string());
    }
    if dictionary.is_empty() {
        return Err(LibreSvipError::msg("empty pinyin dictionary asset"));
    }
    Ok(dictionary)
}

struct PhraseData {
    /// `PHRASES_DICT` reduced to the tone-less first reading of every syllable.
    phrases: HashMap<String, Vec<String>>,
    /// Every prefix of every phrase — `mmseg`'s `PrefixSet`.
    prefixes: HashSet<String>,
}

fn load_phrase_data() -> Result<PhraseData> {
    let tsv = phrases_tsv()
        .ok_or_else(|| LibreSvipError::msg("pinyin phrase asset failed to decompress"))?;
    let mut phrases = HashMap::with_capacity(48_000);
    let mut prefixes: HashSet<String> = HashSet::with_capacity(220_000);
    for line in tsv.lines() {
        let Some((phrase, syllables)) = line.split_once('\t') else {
            continue;
        };
        if phrase.is_empty() {
            continue;
        }
        let mut end = 0;
        for character in phrase.chars() {
            end += character.len_utf8();
            prefixes.insert(phrase[..end].to_string());
        }
        let syllables = syllables.split(' ').map(str::to_string).collect();
        phrases.insert(phrase.to_string(), syllables);
    }
    if phrases.is_empty() {
        return Err(LibreSvipError::msg("empty pinyin phrase asset"));
    }
    Ok(PhraseData { phrases, prefixes })
}

/// A run of Han or non-Han characters, as produced by `pypinyin.seg.simpleseg`.
enum Run<'a> {
    Han(&'a str),
    Other,
}

fn split_runs(text: &str) -> Vec<Run<'_>> {
    let mut runs = Vec::new();
    let mut start = 0;
    let mut current: Option<bool> = None;
    for (index, character) in text.char_indices() {
        let is_han = is_hans_char(character);
        match current {
            None => {
                start = index;
                current = Some(is_han);
            }
            Some(previous) if previous == is_han => {}
            Some(previous) => {
                let run = &text[start..index];
                runs.push(if previous { Run::Han(run) } else { Run::Other });
                start = index;
                current = Some(is_han);
            }
        }
    }
    if let Some(previous) = current {
        let run = &text[start..];
        runs.push(if previous { Run::Han(run) } else { Run::Other });
    }
    runs
}

/// `pypinyin.seg.mmseg.Seg.cut` with `no_non_phrases=True`.
fn mmseg_cut<'a>(text: &'a str, data: &'a PhraseData) -> Vec<&'a str> {
    let mut words = Vec::new();
    let mut remain = text;
    while !remain.is_empty() {
        let mut matched = "";
        let mut broken_at = None;
        for (index, character) in remain.char_indices() {
            let word = &remain[..index + character.len_utf8()];
            if data.prefixes.contains(word) {
                matched = word;
            } else {
                broken_at = Some(index);
                break;
            }
        }
        match broken_at {
            // Every prefix of `remain` is a known phrase prefix.
            None => {
                if data.phrases.contains_key(remain) {
                    words.push(remain);
                } else {
                    let mut end = 0;
                    for character in remain.chars() {
                        let start = end;
                        end += character.len_utf8();
                        words.push(&remain[start..end]);
                    }
                }
                break;
            }
            Some(index) => {
                // A whole phrase ends here: emit it, the rest is cut again.
                if !matched.is_empty() && data.phrases.contains_key(matched) {
                    words.push(matched);
                    remain = &remain[index..];
                } else {
                    // Not a phrase (or nothing matched at all): take one character.
                    let step = remain.chars().next().map_or(0, char::len_utf8);
                    words.push(&remain[..step]);
                    remain = &remain[step..];
                }
            }
        }
    }
    words
}

/// `pypinyin.lazy_pinyin(text, errors=lambda x: " ")`.
///
/// Note that `errors` collapses *every* run of non-Han characters to a single
/// space, exactly like the Python lambda does.
pub fn lazy_pinyin(text: &str) -> Result<Vec<String>> {
    let dictionary = pinyin_dict()?;
    let phrase_data = phrase_data()?;
    let mut pinyin = Vec::new();
    if text.is_empty() {
        // `simple_seg("")` yields one empty word, which `handle_nopinyin` turns
        // into a single space.
        pinyin.push(" ".to_string());
        return Ok(pinyin);
    }
    for run in split_runs(text) {
        match run {
            Run::Han(han) => {
                for word in mmseg_cut(han, phrase_data) {
                    if let Some(syllables) = phrase_data.phrases.get(word) {
                        pinyin.extend(syllables.iter().cloned());
                    } else {
                        for character in word.chars() {
                            pinyin.push(
                                dictionary
                                    .get(&character)
                                    .cloned()
                                    .unwrap_or_else(|| " ".to_string()),
                            );
                        }
                    }
                }
            }
            Run::Other => pinyin.push(" ".to_string()),
        }
    }
    Ok(pinyin)
}

/// The optional flags of `get_pinyin_series`; all of them default to `True`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinyinSeriesOptions {
    pub ignore_hyphens: bool,
    pub reverse_letters: bool,
    pub filter_non_chinese: bool,
}

impl Default for PinyinSeriesOptions {
    fn default() -> Self {
        Self {
            ignore_hyphens: true,
            reverse_letters: true,
            filter_non_chinese: true,
        }
    }
}

/// `libresvip.core.lyric_phoneme.chinese.get_pinyin_series`.
///
/// Letters are reversed (grouped) and every non-Chinese character is dropped by
/// default; syllables of one part are joined with a space when both sides are
/// alphanumeric, and a part that produces no item becomes an empty string.
///
/// Python indexes its item list without bounds checks, so a part containing runs
/// of characters that count as Chinese for `zhon` but are outside pypinyin's
/// `RE_HANS` range (e.g. `U+2A700 U+2A701`) raises `IndexError`; this port
/// returns an error for those inputs instead.
pub fn get_pinyin_series(chinese_series: &[&str]) -> Result<Vec<String>> {
    get_pinyin_series_with(chinese_series, PinyinSeriesOptions::default())
}

/// [`get_pinyin_series`] with the optional flags of the Python signature.
pub fn get_pinyin_series_with(
    chinese_series: &[&str],
    options: PinyinSeriesOptions,
) -> Result<Vec<String>> {
    let part_count = chinese_series.len();
    let mut pinyin_list: Vec<String> = vec![String::new(); part_count];
    let mut item_counts: Vec<usize> = vec![0; part_count];
    let mut result_items: Vec<String> = Vec::new();
    let mut chinese = String::new();

    for (part_index, part) in chinese_series.iter().enumerate() {
        let mut count = 0;
        let mut is_letter = false;
        let mut non_chinese = String::new();
        for character in part.chars() {
            if is_python_space(character) || (options.ignore_hyphens && character == '-') {
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
                    result_items.extend(lazy_pinyin(&chinese)?);
                    chinese.clear();
                }
                if character.is_ascii_alphabetic() {
                    // The Python branch body is nested inside `if reverse_letters`,
                    // so letters are dropped entirely when `reverse_letters` is off.
                    if options.reverse_letters {
                        if !non_chinese.is_empty() && !is_letter {
                            result_items.push(std::mem::take(&mut non_chinese));
                            count += 1;
                        }
                        non_chinese.push(character);
                        is_letter = true;
                    }
                } else if !options.filter_non_chinese {
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
        item_counts[part_index] = count;
    }
    if !chinese.is_empty() {
        result_items.extend(lazy_pinyin(&chinese)?);
    }

    let exhausted = || {
        LibreSvipError::msg(
            "get_pinyin_series: the pinyin list ran out of items (a Han character \
             outside pypinyin's RE_HANS range collapsed a run)",
        )
    };
    let mut pinyin = String::new();
    let mut index = 0;
    for part_index in 0..part_count {
        if item_counts[part_index] == 0 {
            pinyin_list[part_index] = String::new();
            continue;
        }
        pinyin.push_str(result_items.get(index).ok_or_else(exhausted)?);
        index += 1;
        for _ in 1..item_counts[part_index] {
            let previous = result_items.get(index - 1).ok_or_else(exhausted)?;
            let next = result_items.get(index).ok_or_else(exhausted)?;
            let previous_last = previous.chars().last().ok_or_else(exhausted)?;
            let next_first = next.chars().next().ok_or_else(exhausted)?;
            // Python's `str.isalnum`: alphanumeric characters join without a space.
            // `char::is_alphanumeric` additionally accepts the `Other_Alphabetic`
            // combining marks that `str.isalpha` rejects, which can only change
            // the spacing when `filter_non_chinese` is disabled.
            if previous_last.is_alphanumeric() && next_first.is_alphanumeric() {
                pinyin.push(' ');
            }
            pinyin.push_str(next);
            index += 1;
        }
        pinyin_list[part_index] = std::mem::take(&mut pinyin);
    }
    Ok(pinyin_list)
}
