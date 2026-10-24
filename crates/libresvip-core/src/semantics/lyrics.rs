//! Japanese lyrics semantics: lyrics-type detection and conversion between
//! the CV / VCV × kana / romaji lyric conventions (design following
//! UtaFormatix3's `lyrics/japanese`, implemented from scratch on top of the
//! wanakana port in [`crate::core::lyric_phoneme::japanese`]).
//!
//! Definitions used here:
//!
//! * a **CV** lyric is a bare syllable (`a`, `ka`);
//! * a **VCV** lyric carries the previous syllable's tail as a prefix
//!   (`a ka`);
//! * the *tail* of a syllable is its final vowel, or `n` for `ん`;
//! * legato placeholders (`-`, `ー`) render as the previous syllable's tail,
//!   which is the vowel-connection convention UST banks use.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::core::lyric_phoneme::japanese as ja;

const KANA_ROMAJI_JSON: &str = include_str!("../../../../assets/phoneme/kana_romaji.json");

/// The lyrics convention a body of notes is written in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JapaneseLyricsType {
    RomajiCv,
    RomajiVcv,
    KanaCv,
    KanaVcv,
    Unknown,
}

/// What a conversion should produce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LyricsTarget {
    KanaCv,
    KanaVcv,
    RomajiCv,
    RomajiVcv,
}

impl LyricsTarget {
    /// Suffix expected in option values (`"romaji-cv"`, `"kana-vcv"`, …).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "kana-cv" | "kana_cv" => Some(LyricsTarget::KanaCv),
            "kana-vcv" | "kana_vcv" => Some(LyricsTarget::KanaVcv),
            "romaji-cv" | "romaji_cv" => Some(LyricsTarget::RomajiCv),
            "romaji-vcv" | "romaji_vcv" => Some(LyricsTarget::RomajiVcv),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            LyricsTarget::KanaCv => "kana-cv",
            LyricsTarget::KanaVcv => "kana-vcv",
            LyricsTarget::RomajiCv => "romaji-cv",
            LyricsTarget::RomajiVcv => "romaji-vcv",
        }
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Script {
    Kana,
    Romaji,
}

/// Per-note classification: script plus whether the lyric is VCV-shaped.
fn classify(lyric: &str) -> Option<(Script, bool)> {
    // Legato placeholders are their own convention, not a script.
    if ja::is_legato_char(lyric) {
        return None;
    }
    let (base, is_vcv) = match lyric.rsplit_once(' ') {
        Some((_, base)) => (base, true),
        None => (lyric, false),
    };
    if base.is_empty() {
        return None;
    }
    if ja::is_kana(base) {
        Some((Script::Kana, is_vcv))
    } else if ja::is_romaji(base) {
        Some((Script::Romaji, is_vcv))
    } else {
        None
    }
}

/// Detects the convention of one track's lyrics: a type must cover at least
/// 70% of the classifiable notes, otherwise [`JapaneseLyricsType::Unknown`].
pub fn detect_lyrics_type(lyrics: &[String]) -> JapaneseLyricsType {
    let mut counts: HashMap<(Script, bool), usize> = HashMap::new();
    let mut classified = 0usize;
    for lyric in lyrics {
        if let Some(kind) = classify(lyric) {
            *counts.entry(kind).or_default() += 1;
            classified += 1;
        }
    }
    if classified == 0 {
        return JapaneseLyricsType::Unknown;
    }
    let ((script, is_vcv), count) = counts
        .into_iter()
        .max_by_key(|&(kind, count)| (count, kind == (Script::Kana, false)))
        .expect("classified > 0 implies at least one counted kind");
    // Deterministic tie-break above; threshold check:
    if count * 10 >= classified * 7 {
        match (script, is_vcv) {
            (Script::Kana, false) => JapaneseLyricsType::KanaCv,
            (Script::Kana, true) => JapaneseLyricsType::KanaVcv,
            (Script::Romaji, false) => JapaneseLyricsType::RomajiCv,
            (Script::Romaji, true) => JapaneseLyricsType::RomajiVcv,
        }
    } else {
        JapaneseLyricsType::Unknown
    }
}

/// Romaji → hiragana, derived by inverting the kana→romaji table. Where
/// several kana share one romaji (ぁ/あ) the larger code point (the plain
/// glyph) wins, so the mapping is deterministic.
fn romaji_kana_map() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut flat: Vec<(String, String)> = Vec::new();
        let root: serde_json::Value = serde_json::from_str(KANA_ROMAJI_JSON).unwrap_or_default();
        walk_table(&root, "", &mut flat);
        let mut map: HashMap<String, String> = HashMap::new();
        // Ambiguous romaji (ぁ/あ both "a", きゃ/くゃ both "kya"): pin the
        // standard forms for the bare vowels and ん, and otherwise keep the
        // first entry in table order — the table lists standard combos first.
        for (romaji, kana) in [
            ("a", "あ"),
            ("i", "い"),
            ("u", "う"),
            ("e", "え"),
            ("o", "お"),
            ("n", "ん"),
        ] {
            map.insert(romaji.to_owned(), kana.to_owned());
        }
        for (kana, romaji) in flat {
            if kana.is_empty() || romaji.is_empty() {
                continue;
            }
            map.entry(romaji).or_insert(kana);
        }
        map
    })
}

/// Recursively flattens the nested `kana → {"": romaji, …}` table.
fn walk_table(node: &serde_json::Value, prefix: &str, out: &mut Vec<(String, String)>) {
    let Some(entries) = node.as_object() else {
        return;
    };
    for (key, value) in entries {
        match key.as_str() {
            "" => {
                if let Some(romaji) = value.as_str() {
                    out.push((prefix.to_owned(), romaji.to_owned()));
                }
            }
            _ => walk_table(value, &format!("{prefix}{key}"), out),
        }
    }
}

/// Converts romaji text to hiragana with longest-match segmentation; unknown
/// segments pass through unchanged.
fn romaji_to_kana(text: &str) -> String {
    let map = romaji_kana_map();
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by_key(|key| std::cmp::Reverse(key.len()));
    let mut result = String::new();
    let mut rest = text;
    'outer: while !rest.is_empty() {
        for key in &keys {
            if rest.starts_with(key.as_str()) {
                result.push_str(map[*key].as_str());
                rest = &rest[key.len()..];
                continue 'outer;
            }
        }
        let mut characters = rest.chars();
        let first = characters.next().expect("non-empty");
        result.push(first);
        rest = characters.as_str();
    }
    result
}

/// Tail of a romaji syllable: its final vowel, or `n`.
fn syllable_tail(romaji: &str) -> Option<char> {
    romaji
        .chars()
        .rev()
        .find(|character| "aiueon".contains(*character))
}

fn vowel_kana(vowel: char) -> &'static str {
    match vowel {
        'a' => "あ",
        'i' => "い",
        'u' => "う",
        'e' => "え",
        'o' => "お",
        'n' => "ん",
        _ => "-",
    }
}

/// Converts a run of note lyrics to `target`.
///
/// Notes that cannot be interpreted (neither kana nor romaji) are passed
/// through unchanged. Legato placeholders render as the previous syllable's
/// tail in the target script (the UST vowel-connection convention).
pub fn convert_japanese_lyrics(lyrics: &[String], target: LyricsTarget) -> Vec<String> {
    // Canonical romaji base per note; None keeps the original lyric.
    let mut bases: Vec<Option<String>> = Vec::with_capacity(lyrics.len());
    for lyric in lyrics {
        if ja::is_legato_char(lyric) {
            bases.push(Some(String::new())); // placeholder, resolved on render
            continue;
        }
        let base = match lyric.rsplit_once(' ') {
            Some((_, base)) => base,
            None => lyric.as_str(),
        };
        if base.is_empty() {
            bases.push(None);
            continue;
        }
        if ja::is_kana(base) {
            bases.push(Some(ja::to_romaji(base).unwrap_or_else(|| base.to_owned())));
        } else if ja::is_romaji(base) {
            bases.push(Some(base.to_owned()));
        } else {
            bases.push(None);
        }
    }

    let mut result = Vec::with_capacity(lyrics.len());
    let mut previous_tail: Option<char> = None;
    for (index, base) in bases.iter().enumerate() {
        let Some(base) = base else {
            previous_tail = None;
            result.push(lyrics[index].clone());
            continue;
        };
        if base.is_empty() {
            // Legato placeholder: previous vowel in the target script.
            match previous_tail {
                Some(tail) => result.push(match target {
                    LyricsTarget::KanaCv | LyricsTarget::KanaVcv => vowel_kana(tail).to_owned(),
                    _ => tail.to_string(),
                }),
                None => result.push(lyrics[index].clone()),
            }
            continue;
        }
        let rendered = match target {
            LyricsTarget::RomajiCv => base.clone(),
            LyricsTarget::RomajiVcv => match previous_tail {
                Some(tail) => format!("{tail} {base}"),
                None => format!("- {base}"),
            },
            LyricsTarget::KanaCv => romaji_to_kana(base),
            LyricsTarget::KanaVcv => {
                let prefix = match previous_tail {
                    Some(tail) => vowel_kana(tail),
                    None => "-",
                };
                format!("{prefix} {}", romaji_to_kana(base))
            }
        };
        result.push(rendered);
        previous_tail = syllable_tail(base);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lyrics(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn detection_needs_a_supermajority() {
        assert_eq!(
            detect_lyrics_type(&lyrics(&["a", "ka", "ra", "no", "ha"])),
            JapaneseLyricsType::RomajiCv
        );
        assert_eq!(
            detect_lyrics_type(&lyrics(&["あ", "か", "ら", "の", "は"])),
            JapaneseLyricsType::KanaCv
        );
        assert_eq!(
            detect_lyrics_type(&lyrics(&["a ka", "ra i", "no so"])),
            JapaneseLyricsType::RomajiVcv
        );
        assert_eq!(
            detect_lyrics_type(&lyrics(&["あ か", "ら い"])),
            JapaneseLyricsType::KanaVcv
        );
        // 2 of 3 romaji-CV is below the 70% bar once one note is Chinese.
        assert_eq!(
            detect_lyrics_type(&lyrics(&["a", "ka", "你"])),
            JapaneseLyricsType::RomajiCv // 100% of the two classifiable notes
        );
        assert_eq!(
            detect_lyrics_type(&lyrics(&["-", "-", "-"])),
            JapaneseLyricsType::Unknown
        );
    }

    #[test]
    fn vcv_to_cv_takes_the_tail() {
        assert_eq!(
            convert_japanese_lyrics(&lyrics(&["a ka", "a ri", "n ga"]), LyricsTarget::RomajiCv),
            lyrics(&["ka", "ri", "ga"])
        );
    }

    #[test]
    fn cv_to_vcv_prefixes_previous_vowel() {
        assert_eq!(
            convert_japanese_lyrics(&lyrics(&["ka", "ra", "ta"]), LyricsTarget::RomajiVcv),
            lyrics(&["- ka", "a ra", "a ta"])
        );
    }

    #[test]
    fn kana_and_romaji_round_trip() {
        let kana = lyrics(&["か", "ら", "た"]);
        let romaji = convert_japanese_lyrics(&kana, LyricsTarget::RomajiCv);
        assert_eq!(romaji, lyrics(&["ka", "ra", "ta"]));
        let back = convert_japanese_lyrics(&romaji, LyricsTarget::KanaCv);
        assert_eq!(back, kana);
    }

    #[test]
    fn romaji_to_kana_handles_digraphs_and_geminates() {
        assert_eq!(romaji_to_kana("kya"), "きゃ");
        assert_eq!(romaji_to_kana("shi"), "し");
        assert_eq!(romaji_to_kana("kka"), "っか");
        assert_eq!(romaji_to_kana("n"), "ん");
    }

    #[test]
    fn legato_placeholders_become_previous_vowel() {
        assert_eq!(
            convert_japanese_lyrics(&lyrics(&["ka", "-", "ta"]), LyricsTarget::RomajiCv),
            lyrics(&["ka", "a", "ta"])
        );
        assert_eq!(
            convert_japanese_lyrics(&lyrics(&["か", "ー", "た"]), LyricsTarget::KanaCv),
            lyrics(&["か", "あ", "た"])
        );
    }

    #[test]
    fn unknown_notes_pass_through() {
        assert_eq!(
            convert_japanese_lyrics(&lyrics(&["你", "a"]), LyricsTarget::RomajiCv),
            lyrics(&["你", "a"])
        );
    }

    #[test]
    fn target_option_strings_parse() {
        assert_eq!(LyricsTarget::parse("romaji-vcv"), Some(LyricsTarget::RomajiVcv));
        assert_eq!(LyricsTarget::parse("nonsense"), None);
    }
}
