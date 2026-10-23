//! Port of `libresvip.plugins.ds.utils.lyric_util`.
//!
//! `retrie.retrie.Blacklist([...], match_substrings=True).cleanse_text` removes
//! every occurrence of the listed symbols.

const SYMBOLS: [char; 8] = [',', '.', '?', '!', '，', '。', '？', '！'];

/// `get_symbol_removed_lyric`.
pub fn get_symbol_removed_lyric(lyric: &str) -> String {
    if lyric.chars().count() > 1 {
        lyric
            .chars()
            .filter(|character| !SYMBOLS.contains(character))
            .collect()
    } else {
        lyric.to_owned()
    }
}
