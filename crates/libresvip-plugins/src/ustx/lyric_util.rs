//! Port of `libresvip.plugins.ustx.utils.lyric_util`.
//!
//! The Python implementation builds a `retrie.Blacklist` with
//! `right_boundary="$"` and `match_substrings=True`, i.e. the compiled pattern is
//! `(?:,|，|\.|。|\?|？|!|！)(?=$)`: only a punctuation character at the end of
//! the text (or just before a final newline) is matched, and `re.sub` removes a
//! single occurrence.

const PUNCTUATION: [char; 8] = [',', '，', '.', '。', '?', '？', '!', '！'];

/// `is_punctuation`.
pub fn is_punctuation(character: char) -> bool {
    PUNCTUATION.contains(&character)
}

/// `get_symbol_removed_lyric`.
pub fn get_symbol_removed_lyric(lyric: &str) -> String {
    // `$` also matches just before a trailing newline.
    let mut target = lyric;
    let mut suffix = "";
    if let Some(stripped) = lyric.strip_suffix('\n') {
        target = stripped;
        suffix = "\n";
    }
    match target.chars().last() {
        Some(character) if is_punctuation(character) => {
            let trimmed = &target[..target.len() - character.len_utf8()];
            format!("{trimmed}{suffix}")
        }
        _ => lyric.to_owned(),
    }
}
