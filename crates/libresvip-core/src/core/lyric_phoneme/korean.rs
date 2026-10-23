//! Port of `libresvip.core.lyric_phoneme.korean` (the package itself is empty;
//! this module covers `korean.vocaloid_xsampa` and the `ko_pron` romanisation it
//! depends on).
//!
//! `romanise` only implements `system_index="rr"` (Revised Romanisation), the
//! only system used by `libresvip.middlewares.pronounciation_conversion`. The
//! vowel and syllable-boundary tables live in `assets/phoneme/ko_rr.tsv`
//! (see `scripts/gen_phoneme_data.py`).
//!
//! `ko_pron` is not a pure transliterator: it feeds the lyric back through
//! `re.sub(<lyric>, <romanisation>, <lyric>, 1)` once per run of Hangul
//! characters, so regular-expression metacharacters in a lyric decide which part
//! of it is replaced (`"(웃음)"` → `"( useum )"`, `"가|나"` → `"ga na|ga na"`).
//! That is reproduced with the `regex` crate. A lyric whose pattern Python's
//! `re` accepts but the `regex` crate rejects (a literal `{`/`}`, an unknown
//! `\`-escape) returns an error instead of a silently different string.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::error::{LibreSvipError, Result};

/// `assets/phoneme/ko_rr.tsv` — `V\t<jamo>\t<rr>` and `B\t<final>-<initial>\t<rr>`.
const KO_RR_TSV: &str = include_str!("../../../../../assets/phoneme/ko_rr.tsv");

/// `ko_pron.ko_pron.final_syllable_conversion`.
const FINAL_SYLLABLE_CONVERSION: [(&str, &str); 2] = [("", "Ø"), ("X", "")];

/// `romaji2korean_xsampa` from `korean/vocaloid_xsampa.py`.
const ROMAJI2KOREAN_XSAMPA: &[(&str, &str)] = &[
    ("", ""),
    ("a", "a"),
    ("ya", "ja"),
    ("eo", "7"),
    ("yeo", "j7"),
    ("ae", "e"),
    ("e", "e"),
    ("yae", "je"),
    ("ye", "je"),
    ("o", "o"),
    ("wa", "oa"),
    ("yo", "jo"),
    ("u", "u"),
    ("weo", "u7"),
    ("wae", "ue"),
    ("we", "ue"),
    ("oe", "ue"),
    ("wi", "ui"),
    ("eu", "M"),
    ("eui", "Mi"),
    ("i", "i"),
    ("g", "g"),
    ("gg", "g'"),
    ("n", "n"),
    ("d", "d"),
    ("dd", "d'"),
    ("r", "r"),
    ("l", "l"),
    ("m", "m"),
    ("b", "b"),
    ("bb", "b'"),
    ("s", "s"),
    ("ss", "s'"),
    ("j", "c"),
    ("jj", "c'"),
    ("ch", "ch"),
    ("k", "k"),
    ("t", "t"),
    ("p", "p"),
    ("h", "h"),
    ("ng", "Np"),
    ("gs", "gp"),
    ("nch", "np"),
    ("nh", "np"),
    ("kk", "g'"),
    ("tt", "d'"),
    ("pp", "b'"),
    ("wo", "a"),
    ("yu", "ju"),
    ("ui", "i"),
    ("lg", "gp"),
    ("lm", "mp"),
    ("lb", "rp"),
    ("ls", "rp"),
    ("lt", "rp"),
    ("lp", "bp"),
    ("lh", "rp"),
    ("ps", "bp"),
];

/// `romaji2korean_xsampa_final`.
const ROMAJI2KOREAN_XSAMPA_FINAL: &[(&str, &str)] = &[
    ("k", "gp"),
    ("n", "np"),
    ("t", "dp"),
    ("l", "rp"),
    ("m", "mp"),
    ("p", "bp"),
];

fn table_lookup(
    table: &[(&'static str, &'static str)],
    key: &str,
    default: &'static str,
) -> &'static str {
    table
        .iter()
        .find_map(|&(candidate, value)| (candidate == key).then_some(value))
        .unwrap_or(default)
}

/// `ko_pron.ko_pron.system_lookup`
const SYSTEM_LOOKUP: [&str; 6] = ["ph", "rr", "rrr", "mr", "yr", "ipa"];
/// `ko_pron.ko_pron.system_lookup.index("rr")`
const RR: usize = 1;

#[derive(Debug, Clone)]
struct Jamo {
    initial: String,
    vowel: String,
    final_jamo: String,
}

impl Jamo {
    /// The synthetic syllable used for the leading index `-1`.
    fn pseudo_initial() -> Self {
        Self {
            initial: "Ø".to_string(),
            vowel: "Ø".to_string(),
            final_jamo: "X".to_string(),
        }
    }

    /// The synthetic syllable used one past the end of the word.
    fn pseudo_trailing() -> Self {
        Self {
            initial: "Ø".to_string(),
            vowel: "Ø".to_string(),
            final_jamo: "Ø".to_string(),
        }
    }
}

struct KoreanData {
    vowels: HashMap<String, String>,
    boundaries: HashMap<String, String>,
}

static KOREAN_DATA: OnceLock<Result<KoreanData>> = OnceLock::new();

fn korean_data() -> Result<&'static KoreanData> {
    match KOREAN_DATA.get_or_init(load_korean_data) {
        Ok(data) => Ok(data),
        Err(error) => Err(LibreSvipError::msg(format!(
            "ko_pron tables are unavailable: {error}"
        ))),
    }
}

fn load_korean_data() -> Result<KoreanData> {
    let mut vowels = HashMap::new();
    let mut boundaries = HashMap::new();
    for line in KO_RR_TSV.lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let (Some(kind), Some(key), value) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let Some(value) = value else { continue };
        match kind {
            "V" => {
                vowels.insert(key.to_string(), value.to_string());
            }
            "B" => {
                boundaries.insert(key.to_string(), value.to_string());
            }
            _ => {}
        }
    }
    if vowels.is_empty() || boundaries.is_empty() {
        return Err(LibreSvipError::msg("empty ko_pron table asset"));
    }
    Ok(KoreanData { vowels, boundaries })
}

/// `ko_pron.ko_pron.decompose_jamo`.
fn decompose_jamo(character: char) -> Jamo {
    let code = character as u32;
    if (0xAC00..=0xD7A3).contains(&code) {
        // Hangul syllable: choseong / jungseong / jongseong.
        let relative = code - 0xAC00;
        let jongseong = relative % 28;
        let jungseong = (relative % 588) / 28;
        let choseong = relative / 588;
        return Jamo {
            initial: char::from_u32(0x1100 + choseong)
                .unwrap_or('\u{1100}')
                .to_string(),
            vowel: char::from_u32(0x1161 + jungseong)
                .unwrap_or('\u{1161}')
                .to_string(),
            final_jamo: if jongseong == 0 {
                String::new()
            } else {
                char::from_u32(0x11A7 + jongseong)
                    .unwrap_or('\u{11A8}')
                    .to_string()
            },
        };
    }
    if (0x1100..=0x1112).contains(&code) {
        Jamo {
            initial: character.to_string(),
            vowel: "Ø".to_string(),
            final_jamo: "Ø".to_string(),
        }
    } else if (0x1161..=0x1175).contains(&code) {
        Jamo {
            initial: "Ø".to_string(),
            vowel: character.to_string(),
            final_jamo: "Ø".to_string(),
        }
    } else if (0x11A8..=0x11C2).contains(&code) || (0x3131..=0x318E).contains(&code) {
        Jamo {
            initial: "Ø".to_string(),
            vowel: "Ø".to_string(),
            final_jamo: character.to_string(),
        }
    } else {
        Jamo {
            initial: "Ø".to_string(),
            vowel: " ".to_string(),
            final_jamo: "X".to_string(),
        }
    }
}

fn convert_final(final_jamo: &str) -> String {
    FINAL_SYLLABLE_CONVERSION
        .iter()
        .find_map(|&(from, to)| (from == final_jamo).then(|| to.to_string()))
        .unwrap_or_else(|| final_jamo.to_string())
}

/// Membership in `ko_pron`'s character class
/// `[ᄀ-하-ᅵᆨ-ᇂㄱ-ㆎ가-힣' ]`.
fn is_transliterable(character: char) -> bool {
    let code = character as u32;
    (0x1100..=0x1112).contains(&code)
        || (0x1161..=0x1175).contains(&code)
        || (0x11A8..=0x11C2).contains(&code)
        || (0x3131..=0x318E).contains(&code)
        || (0xAC00..=0xD7A3).contains(&code)
        || character == '\''
        || character == ' '
}

/// How many matches `re.finditer("[...]+", text)` yields: one per run of
/// transliterable characters.
fn transliterable_runs(characters: &[char]) -> usize {
    let mut runs = 0;
    let mut inside = false;
    for character in characters {
        let allowed = is_transliterable(*character);
        if allowed && !inside {
            runs += 1;
        }
        inside = allowed;
    }
    runs
}

/// `re.sub('["-%](.)', "\\1", text)` — drops `"`, `#`, `$` and `%` when followed
/// by another (non-newline) character.
fn strip_quoted_characters(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut stripped = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        let is_prefix = matches!(character, '"' | '#' | '$' | '%');
        if is_prefix && index + 1 < characters.len() && characters[index + 1] != '\n' {
            stripped.push(characters[index + 1]);
            index += 2;
        } else {
            stripped.push(character);
            index += 1;
        }
    }
    stripped
}

/// `ko_pron.ko_pron.romanise(word, system_index="rr")` — Revised Romanisation.
pub fn romanise_rr(word: &str) -> Result<String> {
    romanise(word, "rr")
}

/// `ko_pron.ko_pron.romanise`.
///
/// Only `system_index = "rr"` is ported; the other systems (`ph`, `rrr`, `mr`,
/// `yr`, `ipa`) return [`LibreSvipError::NotImplemented`].
pub fn romanise(word: &str, system_index: &str) -> Result<String> {
    if !SYSTEM_LOOKUP.contains(&system_index) {
        return Err(LibreSvipError::msg(format!(
            "unknown romanisation system: {system_index}"
        )));
    }
    if SYSTEM_LOOKUP.iter().position(|system| *system == system_index) != Some(RR) {
        return Err(LibreSvipError::NotImplemented);
    }
    let dictionary = korean_data()?;
    let text = strip_quoted_characters(word);
    let characters: Vec<char> = text.chars().collect();
    let runs = transliterable_runs(&characters);
    if runs == 0 {
        // `re.finditer` never matches, so the input is returned untouched.
        return Ok(text);
    }
    let romanisation = romanise_characters(&characters, dictionary)?;
    if !text.chars().any(is_regex_metacharacter) {
        // Without metacharacters the pattern is the literal text, so the first
        // match is the whole string and it is substituted exactly once.
        return Ok(romanisation);
    }
    // `ko_pron` substitutes with `re.sub(primitive_word, romanisation, text, 1)`
    // once per match of the character-class `finditer`, i.e. once per run of
    // Hangul characters. Metacharacters in the text therefore keep whatever the
    // pattern does not match.
    let pattern = regex::Regex::new(&text).map_err(|error| {
        // Python raises `re.error` for the same inputs (unbalanced groups/sets).
        LibreSvipError::msg(format!("invalid ko_pron substitution pattern {text:?}: {error}"))
    })?;
    let mut replaced = text.clone();
    for _ in 0..runs {
        let Some(found) = pattern.find(&replaced) else {
            break;
        };
        let mut next = String::with_capacity(replaced.len() + romanisation.len());
        next.push_str(&replaced[..found.start()]);
        next.push_str(&romanisation);
        next.push_str(&replaced[found.end()..]);
        replaced = next;
    }
    Ok(replaced)
}

/// The body of `romanise` for one string: no regex, no text substitution.
fn romanise_characters(characters: &[char], dictionary: &KoreanData) -> Result<String> {
    let syllables: Vec<Jamo> = characters.iter().map(|&c| decompose_jamo(c)).collect();
    let syllable_count = syllables.len() as isize;
    let mut romanisation = String::new();
    for index in -1..syllable_count {
        let mut syllable = if index >= 0 {
            syllables[index as usize].clone()
        } else {
            Jamo::pseudo_initial()
        };
        let next_syllable = if index + 1 < syllable_count {
            syllables[(index + 1) as usize].clone()
        } else {
            Jamo::pseudo_trailing()
        };
        let this_character = if index >= 0 {
            characters[index as usize]
        } else {
            '\0'
        };
        syllable.final_jamo = convert_final(&syllable.final_jamo);

        let vowel = dictionary
            .vowels
            .get(&syllable.vowel)
            .cloned()
            .ok_or_else(|| LibreSvipError::msg(format!("No vowel data for {}.", syllable.vowel)))?;

        // `system_index not in {2, 4}`: the rr-specific final adjustments.
        if index >= 0 && this_character == '밟' {
            syllable.final_jamo = "ᆸ".to_string();
        }
        // `final_next_syllable = syllable['final'] + respelling[index + 1:index + 2]`
        let next_character = characters.get((index + 1).max(0) as usize).copied();
        if let (Some(final_char), Some(next_character)) =
            (syllable.final_jamo.chars().next(), next_character)
        {
            if (final_char == 'ᇀ' || final_char == 'ᆴ')
                && (next_character == '이' || next_character == '히')
            {
                syllable.final_jamo = "ᆾ".to_string();
            } else if final_char == 'ᆮ' && next_character == '이' {
                syllable.final_jamo = "ᆽ".to_string();
            } else if final_char == 'ᆮ' && next_character == '히' {
                syllable.final_jamo = "ᆾ".to_string();
            }
        }

        let boundary_key = format!("{}-{}", syllable.final_jamo, next_syllable.initial);
        let junction = dictionary.boundaries.get(&boundary_key).ok_or_else(|| {
            LibreSvipError::msg(format!("No boundary data for {boundary_key}."))
        })?;
        romanisation.push_str(&vowel);
        romanisation.push_str(junction);
    }
    // `system_index in {1, 2}`: the `(.)…(.)` pass runs twice because a single
    // pass cannot consume two boundaries that share a character. The `…` comes
    // from the `Ø-ᄋ` boundary (a syllable without a final followed by a silent
    // initial); `ambiguous_intersyllabic_rr` pairs keep a hyphen between them.
    for _ in 0..2 {
        romanisation = collapse_elision(&romanisation);
    }
    Ok(romanisation)
}

/// Characters that are special to both Python's `re` and the `regex` crate.
fn is_regex_metacharacter(character: char) -> bool {
    matches!(
        character,
        '\\' | '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
    )
}

/// One `re.sub("(.)…(.)", substitute, text)` pass of `ko_pron.romanise`.
fn collapse_elision(romanisation: &str) -> String {
    let characters: Vec<char> = romanisation.chars().collect();
    let mut collapsed = String::with_capacity(romanisation.len());
    let mut index = 0;
    while index < characters.len() {
        if index + 2 < characters.len() && characters[index + 1] == '…' {
            let left = characters[index];
            let right = characters[index + 2];
            collapsed.push(left);
            // `ambiguous_intersyllabic_rr` = {"oe", "eo", "eu", "ae", "ui"}
            if matches!(
                [left, right],
                ['o', 'e'] | ['e', 'o'] | ['e', 'u'] | ['a', 'e'] | ['u', 'i']
            ) {
                collapsed.push('-');
            }
            collapsed.push(right);
            index += 3;
        } else {
            collapsed.push(characters[index]);
            index += 1;
        }
    }
    collapsed
}

/// `libresvip.core.lyric_phoneme.korean.vocaloid_xsampa.hangul2xsampa`.
pub fn hangul2xsampa(lyric: &str) -> Result<String> {
    let mut phonemes = Vec::new();
    for character in lyric.chars() {
        let syllable = decompose_jamo(character);
        let initial_romaji = romanise_rr(&syllable.initial)?;
        let vowel_romaji = romanise_rr(&syllable.vowel)?;
        let final_romaji = romanise_rr(&syllable.final_jamo)?;
        let mut initial_xsampa = table_lookup(&ROMAJI2KOREAN_XSAMPA, &initial_romaji, "");
        let mut vowel_xsampa = table_lookup(&ROMAJI2KOREAN_XSAMPA, &vowel_romaji, "");
        let mut final_xsampa = table_lookup(&ROMAJI2KOREAN_XSAMPA, &final_romaji, "");
        if matches!(initial_xsampa, "s" | "sh")
            && (vowel_xsampa.starts_with('i')
                || vowel_xsampa.starts_with('y')
                || vowel_xsampa.starts_with('j'))
        {
            initial_xsampa = if initial_xsampa == "s" { "sh" } else { "sh'" };
        } else if matches!(vowel_xsampa, "s" | "sh")
            && (final_xsampa.starts_with('i')
                || final_xsampa.starts_with('y')
                || final_xsampa.starts_with('j'))
        {
            vowel_xsampa = if vowel_xsampa == "s" { "sh" } else { "sh'" };
        }
        final_xsampa = table_lookup(&ROMAJI2KOREAN_XSAMPA_FINAL, final_xsampa, final_xsampa);
        phonemes.push(
            format!("{initial_xsampa} {vowel_xsampa} {final_xsampa}")
                .trim()
                .to_string(),
        );
    }
    if phonemes.is_empty() {
        Ok("r a".to_string())
    } else {
        Ok(phonemes.join(" "))
    }
}
