//! Port of `libresvip.plugins.ds.utils.pinyin_util` and `phoneme_dict`.

use std::collections::HashMap;

use libresvip_core::core::lyric_phoneme::chinese::get_pinyin_series;
use libresvip_core::error::{LibreSvipError, Result};

/// `dicts/opencpop-extension.txt`.
const OPENC_POP_DICT: &str = include_str!("dicts/opencpop-extension.txt");
/// `dicts/japanese_dict_full.txt`.
const JAPANESE_DICT: &str = include_str!("dicts/japanese_dict_full.txt");

/// `get_opencpop_dict(dict_name)`: a `pinyin -> phonemes` table.
///
/// The Python reader is a `csv.DictReader` with tab delimiters and the field
/// names `pinyin`/`phone`.
pub fn load_phoneme_table(dict_name: &str) -> Result<HashMap<String, String>> {
    let content = match dict_name {
        "opencpop-extension" => OPENC_POP_DICT,
        "japanese_dict_full" => JAPANESE_DICT,
        _ => return Err(LibreSvipError::msg("Cannot find dict.")),
    };
    let mut table = HashMap::new();
    for line in content.lines() {
        // `csv` skips empty lines; every row of both dictionaries has two fields.
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let Some(pinyin) = fields.next() else {
            continue;
        };
        let phone = fields.next().unwrap_or_default();
        table.insert(pinyin.to_owned(), phone.to_owned());
    }
    Ok(table)
}

/// `pinyin_util.split`.
pub fn split_pinyin(table: &HashMap<String, String>, pinyin: &str) -> Result<(String, String)> {
    let Some(phonemes) = table.get(pinyin) else {
        return Err(LibreSvipError::msg(format!(
            "The selected dictionary does not contain the pronunciation \"{pinyin}\". \
             Please check the pronunciation or try another dictionary."
        )));
    };
    let phonemes: Vec<&str> = phonemes.split_whitespace().collect();
    if phonemes.len() < 2 {
        Ok((String::new(), phonemes.first().copied().unwrap_or("").to_owned()))
    } else {
        Ok((phonemes[0].to_owned(), phonemes[1].to_owned()))
    }
}

/// `add_pinyin_from_lyrics` — the tone-less pinyin of every lyric, in order.
pub fn pinyin_series(lyrics: &[String]) -> Result<Vec<String>> {
    let parts: Vec<&str> = lyrics.iter().map(String::as_str).collect();
    get_pinyin_series(&parts)
}
