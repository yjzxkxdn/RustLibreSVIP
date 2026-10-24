//! Cantonese (Jyutping) romanisation — the Rust counterpart of the Python
//! `jyutping` package as used by `libresvip` (single reading per character).
//!
//! The dictionary (開放粵語字典, CC-BY 3.0, shipped inside the `jyutping`
//! package) is embedded zlib-compressed; see
//! `assets/phoneme/jyutping.tsv` / `scripts/gen_phoneme_data.py`.
//!
//! Known difference: Python picks the single reading with `set.pop()`, which
//! depends on the per-process string hash seed; this port always picks the
//! alphabetically first reading so output is deterministic.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::error::{LibreSvipError, Result};

const JYUTPING_TSV_ZLIB: &[u8] =
    include_bytes!("../../../../../assets/phoneme/jyutping.tsv.zlib");

fn jyutping_tsv() -> Result<&'static str> {
    static TABLE: OnceLock<Result<String>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            crate::compression::zlib_decompress(JYUTPING_TSV_ZLIB).and_then(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|_| LibreSvipError::msg("jyutping table is not UTF-8"))
            })
        })
        .as_deref()
        .map_err(|error| {
            LibreSvipError::msg(format!(
                "jyutping table asset failed to decompress: {error}"
            ))
        })
}

fn dictionary() -> Result<&'static HashMap<char, String>> {
    static DICT: OnceLock<Result<HashMap<char, String>>> = OnceLock::new();
    DICT.get_or_init(|| {
        let mut map = HashMap::with_capacity(8_000);
        for line in jyutping_tsv()?.lines() {
            if line.starts_with('#') {
                continue;
            }
            let Some((character, reading)) = line.split_once('\t') else {
                continue;
            };
            let mut characters = character.chars();
            let (Some(character), None) = (characters.next(), characters.next()) else {
                continue;
            };
            map.insert(character, reading.to_owned());
        }
        if map.is_empty() {
            return Err(LibreSvipError::msg("empty jyutping table asset"));
        }
        Ok(map)
    })
    .as_ref()
    .map_err(|error| LibreSvipError::msg(format!("jyutping dictionary is unavailable: {error}")))
}

/// `jyutping.get_jyutping(text)` with a single deterministic reading:
/// one entry per character, `None` for characters without a reading.
pub fn get_jyutping(text: &str) -> Result<Vec<Option<String>>> {
    let dict = dictionary()?;
    Ok(text
        .chars()
        .map(|character| dict.get(&character).cloned())
        .collect())
}

/// The call-site shape used by `sv_g2p_one`: readings joined with spaces,
/// tone digits dropped (`re.sub(r"\d+$", " ", each)`), empty results falling
/// back to the original input.
pub fn toneless_or_original(text: &str) -> Result<String> {
    let readings = get_jyutping(text)?;
    let joined = readings
        .into_iter()
        .map(|reading| {
            reading
                .map(|reading| {
                    let trimmed = reading.trim_end_matches(|c: char| c.is_ascii_digit());
                    format!("{trimmed} ")
                })
                .unwrap_or_default()
        })
        .collect::<String>();
    let joined = joined.trim();
    if joined.is_empty() {
        Ok(text.to_owned())
    } else {
        Ok(joined.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_readings_round_trip() {
        assert_eq!(
            get_jyutping("你好").expect("dict"),
            vec![
                Some("nei5".to_owned()),
                Some("hou2".to_owned())
            ]
        );
    }

    #[test]
    fn toneless_join_matches_the_svp_call_site() {
        assert_eq!(toneless_or_original("你好").expect("dict"), "nei hou");
        assert_eq!(toneless_or_original("hello").expect("dict"), "hello");
    }
}
