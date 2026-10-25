//! User-editable phoneme mapping middleware (T7 of the UtaFormatix3
//! absorption plan).
//!
//! The mapping table is plain text, one `from = to` rule per line, tokens
//! separated by spaces. Rules are applied by longest match: an entry with
//! more tokens (then more characters) wins, so multi-phoneme rules take
//! priority over single ones. Rust-port addition — Python LibreSVIP has no
//! equivalent; the built-in tables stay untouched.

use std::collections::HashMap;

use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Project, Track};
use libresvip_core::options::{EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{Middleware, PluginInfo};

pub static INFO_PHONEME_MAPPING: PluginInfo = PluginInfo {
    id: "phoneme_mapping",
    name: "Phoneme Mapping",
    author: "依旧在星空下等你",
    website: "https://github.com/yjzxkxdn/RustLibreSVIP",
    file_format: "",
    description: "Rewrite note pronunciations through a user-supplied `from = to` table (longest match first), e.g. `sh a = s a`.",
    suffixes: &[],
    icon_base64: None,
};

/// One `from = to` rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappingRule {
    pub from: Vec<String>,
    pub to: Vec<String>,
}

/// Parses the textual mapping table; invalid lines are skipped with a
/// collected warning.
pub fn parse_mapping(text: &str) -> (Vec<MappingRule>, Vec<String>) {
    let mut rules = Vec::new();
    let mut problems = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        match line.split_once('=') {
            Some((from, to)) => {
                let from: Vec<String> = from.split_whitespace().map(str::to_owned).collect();
                let to: Vec<String> = to.split_whitespace().map(str::to_owned).collect();
                if from.is_empty() {
                    problems.push(format!("line {}: empty source", number + 1));
                    continue;
                }
                rules.push(MappingRule { from, to });
            }
            None => problems.push(format!("line {}: expected `from = to`", number + 1)),
        }
    }
    // Longest-match ordering: more tokens first, then more characters.
    rules.sort_by(|left, right| {
        let key = |rule: &MappingRule| (rule.from.len(), rule.from.join(" ").len());
        key(right).cmp(&key(left))
    });
    (rules, problems)
}

/// Applies the rules to one phoneme string (space-separated tokens).
/// Unmatched tokens pass through unchanged.
pub fn apply_mapping(phonemes: &str, rules: &[MappingRule]) -> String {
    if rules.is_empty() {
        return phonemes.to_owned();
    }
    let tokens: Vec<&str> = phonemes.split_whitespace().collect();
    // Rules grouped by token count; within a group the sort order (longest
    // match first) decides.
    let mut by_length: HashMap<usize, Vec<&MappingRule>> = HashMap::new();
    for rule in rules {
        by_length.entry(rule.from.len()).or_default().push(rule);
    }
    let max_len = rules.first().map(|rule| rule.from.len()).unwrap_or(1);
    let mut output: Vec<String> = Vec::with_capacity(tokens.len());
    let mut index = 0usize;
    'tokens: while index < tokens.len() {
        for length in (1..=max_len.min(tokens.len() - index)).rev() {
            let Some(group) = by_length.get(&length) else {
                continue;
            };
            for rule in group {
                if tokens[index..index + length] == rule.from[..] {
                    output.extend(rule.to.iter().cloned());
                    index += length;
                    continue 'tokens;
                }
            }
        }
        output.push(tokens[index].to_owned());
        index += 1;
    }
    output.join(" ")
}

const APPLY_TO_VARIANTS: &[EnumVariant] = &[
    EnumVariant { name: "PRONUNCIATION", value: "pronunciation", title: "Note pronunciations" },
    EnumVariant { name: "LYRIC", value: "lyric", title: "Note lyrics" },
    EnumVariant { name: "BOTH", value: "both", title: "Both" },
];

pub struct PhonemeMappingMiddleware;

impl Middleware for PhonemeMappingMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_PHONEME_MAPPING
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::string("mapping", "Mapping table", "")
                .with_description(
                    "One `from = to` rule per line; tokens separated by spaces. \
                     Lines starting with # are comments.",
                ),
            OptionFieldDef::enumerated(
                "apply_to",
                "Apply to",
                "PhonemeMappingField",
                APPLY_TO_VARIANTS,
                "PRONUNCIATION",
            )
            .with_description("Which field the table is applied to."),
        ]
    }

    fn process(&self, mut project: Project, options: &Options) -> Result<Project> {
        let mapping_text = options.str("mapping", "");
        if mapping_text.trim().is_empty() {
            return Ok(project);
        }
        let (rules, problems) = parse_mapping(mapping_text.as_str());
        for problem in problems {
            show_warning_kind(WarningKind::Other, format!("phoneme mapping: {problem}"));
        }
        if rules.is_empty() {
            return Err(LibreSvipError::msg("phoneme mapping: no valid rules"));
        }
        let apply_to = options.str("apply_to", "pronunciation");
        let touch_pronunciation = apply_to == "pronunciation" || apply_to == "both";
        let touch_lyric = apply_to == "lyric" || apply_to == "both";
        for track in &mut project.track_list {
            if let Track::Singing(singing) = track {
                for note in &mut singing.note_list {
                    if touch_pronunciation {
                        if let Some(pronunciation) = note.pronunciation.clone() {
                            note.pronunciation = Some(apply_mapping(&pronunciation, &rules));
                        }
                    }
                    if touch_lyric {
                        note.lyric = apply_mapping(&note.lyric, &rules);
                    }
                }
            }
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_token_rules_win() {
        let (rules, problems) = parse_mapping("sh a = s a\na = b\n# comment\n");
        assert!(problems.is_empty());
        assert_eq!(rules.len(), 2);
        assert_eq!(apply_mapping("sh a ka", &rules), "s a ka");
        assert_eq!(apply_mapping("a ka", &rules), "b ka");
        assert_eq!(apply_mapping("ka", &rules), "ka");
    }

    #[test]
    fn empty_replacements_delete_tokens() {
        let (rules, _) = parse_mapping("cl = ");
        assert_eq!(apply_mapping("a cl b", &rules), "a b");
    }

    #[test]
    fn invalid_lines_warn_but_do_not_fail() {
        let (rules, problems) = parse_mapping("no equals sign\n= backwards\nok = fine");
        assert_eq!(rules.len(), 1);
        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn empty_table_is_identity() {
        let (rules, _) = parse_mapping("");
        assert!(rules.is_empty());
        assert_eq!(apply_mapping("a b", &rules), "a b");
    }
}
