//! Differential tests for the lyric/phoneme helpers, driven by
//! `scripts/check_phoneme.py` (Python reference implementation).
//!
//! `expected: null` in the corpus means the Python implementation raised; the
//! Rust side must report an error / `None` for those inputs instead of panicking.

use libresvip_core::core::lyric_phoneme::chinese::{self, PinyinSeriesOptions};
use libresvip_core::core::lyric_phoneme::{japanese, korean};
use serde_json::Value;

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn cases() -> Value {
    let path = workspace_root().join("testdata").join("phoneme_cases.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "read {}: {error} (run scripts/check_phoneme.py)",
            path.display()
        )
    });
    serde_json::from_slice(&bytes).expect("parse phoneme_cases.json")
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn options(value: &Value) -> PinyinSeriesOptions {
    let Value::Object(flags) = value else {
        return PinyinSeriesOptions::default();
    };
    let flag = |name: &str| {
        flags
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(true)
    };
    PinyinSeriesOptions {
        ignore_hyphens: flag("ignore_hyphens"),
        reverse_letters: flag("reverse_letters"),
        filter_non_chinese: flag("filter_non_chinese"),
    }
}

#[test]
fn pinyin_characters() {
    let data = cases();
    let cases = data["pinyin_characters"].as_array().expect("cases");
    assert!(cases.len() > 40_000, "corpus is suspiciously small");
    for case in cases {
        let character = case[0].as_str().expect("character");
        let lazy = strings(&case[1]);
        let series = strings(&case[2]);
        assert_eq!(
            chinese::lazy_pinyin(character).expect("lazy_pinyin"),
            lazy,
            "lazy_pinyin({character:?})"
        );
        assert_eq!(
            chinese::get_pinyin_series(&[character]).expect("get_pinyin_series"),
            series,
            "get_pinyin_series([{character:?}])"
        );
    }
}

#[test]
fn lazy_pinyin_texts() {
    let data = cases();
    for case in data["lazy_pinyin"].as_array().expect("cases") {
        let text = case[0].as_str().expect("text");
        let expected = strings(&case[1]);
        assert_eq!(
            chinese::lazy_pinyin(text).expect("lazy_pinyin"),
            expected,
            "lazy_pinyin({text:?})"
        );
    }
}

#[test]
fn pinyin_series() {
    let data = cases();
    for case in data["pinyin_series"].as_array().expect("cases") {
        let parts: Vec<&str> = case[0]
            .as_array()
            .expect("parts")
            .iter()
            .map(|part| part.as_str().expect("part"))
            .collect();
        let flags = options(&case[1]);
        let actual = chinese::get_pinyin_series_with(&parts, flags);
        match &case[2] {
            Value::Null => assert!(
                actual.is_err(),
                "get_pinyin_series({parts:?}) should fail like Python, got {actual:?}"
            ),
            expected => assert_eq!(
                actual.expect("get_pinyin_series"),
                strings(expected),
                "get_pinyin_series({parts:?}, {flags:?})"
            ),
        }
    }
}

#[test]
fn random_lyrics() {
    let data = cases();
    for case in data["random_lyrics"].as_array().expect("cases") {
        let text = case[0].as_str().expect("text");
        assert_eq!(
            chinese::lazy_pinyin(text).expect("lazy_pinyin"),
            strings(&case[1]),
            "lazy_pinyin({text:?})"
        );
        let parts: Vec<&str> = case[2]
            .as_array()
            .expect("parts")
            .iter()
            .map(|part| part.as_str().expect("part"))
            .collect();
        let actual = chinese::get_pinyin_series(&parts);
        match &case[3] {
            Value::Null => assert!(
                actual.is_err(),
                "get_pinyin_series({parts:?}) should fail like Python, got {actual:?}"
            ),
            expected => assert_eq!(
                actual.expect("get_pinyin_series"),
                strings(expected),
                "get_pinyin_series({parts:?})"
            ),
        }
    }
}

#[test]
fn romaji() {
    let data = cases();
    for case in data["to_romaji"].as_array().expect("cases") {
        let text = case[0].as_str().expect("text");
        let actual = japanese::to_romaji(text);
        match &case[1] {
            Value::Null => assert!(
                actual.is_none(),
                "to_romaji({text:?}) should fail like Python, got {actual:?}"
            ),
            expected => assert_eq!(
                actual.as_deref(),
                Some(expected.as_str().expect("romaji")),
                "to_romaji({text:?})"
            ),
        }
    }
}

#[test]
fn korean_romanisation() {
    let data = cases();
    for case in data["korean"].as_array().expect("cases") {
        let text = case[0].as_str().expect("text");
        let actual = korean::romanise_rr(text);
        match &case[1] {
            Value::Null => assert!(
                actual.is_err(),
                "romanise_rr({text:?}) should fail like Python, got {actual:?}"
            ),
            expected => assert_eq!(
                actual.expect("romanise_rr"),
                expected.as_str().expect("romanisation"),
                "romanise_rr({text:?})"
            ),
        }
    }
}

#[test]
fn korean_xsampa() {
    let data = cases();
    for case in data["korean_xsampa"].as_array().expect("cases") {
        let text = case[0].as_str().expect("text");
        let actual = korean::hangul2xsampa(text);
        match &case[1] {
            Value::Null => assert!(
                actual.is_err(),
                "hangul2xsampa({text:?}) should fail like Python, got {actual:?}"
            ),
            expected => assert_eq!(
                actual.expect("hangul2xsampa"),
                expected.as_str().expect("xsampa"),
                "hangul2xsampa({text:?})"
            ),
        }
    }
}
