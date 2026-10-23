//! Character classes shared by the `mid`, `lrc` and `srt` plugins.
//!
//! Port of the two regexes in `libresvip.utils.text`:
//!
//! ```python
//! LATIN_ALPHABET = re.compile(r"[a-zA-Z]+")
//! SYMBOL_PATTERN = re.compile(rf"(?!-)[\!\"\#\$%\&'\(\)\*,\./:;<=>\?\[\\\]\^_`\{{\|\}}\~{zhon.hanzi.punctuation}]+")
//! ```
//!
//! [`SYMBOL_PATTERN`] is a plain character class repeated once or more (the
//! `(?!-)` lookahead is redundant because `-` is not a member), so a set
//! membership test is byte-for-byte equivalent while avoiding a compiled
//! regex and the unsupported lookaround.
//!
//! This module lives under `mid` because the plugin registry in `lib.rs` is
//! shared; `lrc` and `srt` import it as `crate::mid::text`.

/// `zhon.hanzi.punctuation` plus the ASCII punctuation listed in the pattern.
fn is_symbol_char(character: char) -> bool {
    let code = character as u32;
    matches!(
        code,
        // ASCII: ! " # $ % & ' ( ) * , . / : ; < = > ? [ \ ] ^ _ ` { | } ~
        // (`+`, `-` and `@` are deliberately absent, matching Python.)
        0x0021..=0x002A
            | 0x002C
            | 0x002E..=0x002F
            | 0x003A..=0x003F
            | 0x005B..=0x0060
            | 0x007B..=0x007E
            // zhon.hanzi.punctuation
            | 0x00B7
            | 0x2013
            | 0x2014
            | 0x2018
            | 0x2019
            | 0x201B..=0x201F
            | 0x2026
            | 0x2027
            | 0x3000..=0x3003
            | 0x3008..=0x3011
            | 0x3014..=0x301F
            | 0x3030
            | 0x303E
            | 0x303F
            | 0xFE4F
            | 0xFE51
            | 0xFE54
            | 0xFF01..=0xFF0F
            | 0xFF1A..=0xFF20
            | 0xFF3B..=0xFF40
            | 0xFF5B..=0xFF64
    )
}

/// `SYMBOL_PATTERN.search(text) is not None`.
pub fn contains_symbol(text: &str) -> bool {
    text.chars().any(is_symbol_char)
}

/// `SYMBOL_PATTERN.sub("", text)`.
pub fn remove_symbols(text: &str) -> String {
    text.chars().filter(|character| !is_symbol_char(*character)).collect()
}

/// `LATIN_ALPHABET.search(text) is not None`.
pub fn contains_latin(text: &str) -> bool {
    text.chars().any(|character| character.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_class_matches_python() {
        for character in "!\"#$%&'()*,./:;<=>?[\\]^_`{|}~".chars() {
            assert!(is_symbol_char(character), "{character:?} is a symbol");
        }
        for character in "+-@ abcZ0".chars() {
            assert!(!is_symbol_char(character), "{character:?} is not a symbol");
        }
        assert!(is_symbol_char('\u{3002}'));
        assert!(is_symbol_char('\u{FF01}'));
        assert!(!is_symbol_char('\u{4E00}'));
    }

    #[test]
    fn latin_and_symbols() {
        assert!(contains_latin("la"));
        assert!(!contains_latin("啦"));
        assert_eq!(remove_symbols("la, li."), "la li");
        assert!(contains_symbol("你好！"));
    }
}
