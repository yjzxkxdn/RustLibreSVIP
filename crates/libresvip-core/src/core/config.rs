//! Port of `libresvip.core.config`.
//!
//! Settings live in `settings.yml` under the platform config directory. The
//! Python implementation round-trips that file through PyYAML with
//! `sort_keys=True`, so the emitter below reproduces PyYAML's block style
//! (including the `!!python/object/apply:pathlib.WindowsPath` tag for paths).

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};

use indexmap::IndexMap;
use serde_yaml::Value;

use crate::core::constants::app_dirs;
use crate::error::{IoContext, LibreSvipError, Result};

pub const SETTINGS_FILENAME: &str = "settings.yml";

// ---------------------------------------------------------------------------
// Enums (serialised by *name*, as `pydantic_enum` does)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Chinese,
    English,
    Japanese,
    German,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::Chinese => "CHINESE",
            Language::English => "ENGLISH",
            Language::Japanese => "JAPANESE",
            Language::German => "GERMAN",
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            Language::Chinese => "zh_CN",
            Language::English => "en_US",
            Language::Japanese => "ja_JP",
            Language::German => "de_DE",
        }
    }

    /// `Language.to_language("zh_CN")` → `"zh-cn"`.
    fn to_language(locale: &str) -> String {
        match locale.find('_') {
            Some(position) => format!(
                "{}-{}",
                locale[..position].to_lowercase(),
                locale[position + 1..].to_lowercase()
            ),
            None => locale.to_lowercase(),
        }
    }

    /// `Language.from_locale`.
    pub fn from_locale(locale: &str) -> Language {
        match Language::to_language(locale).as_str() {
            "zh-cn" => Language::Chinese,
            "ja-jp" => Language::Japanese,
            "de-de" => Language::German,
            _ => Language::English,
        }
    }

    /// `Language.auto()` — system UI language.
    pub fn auto() -> Language {
        #[cfg(target_os = "windows")]
        {
            if let Some(locale) = windows_ui_locale() {
                return Language::from_locale(&locale);
            }
        }
        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(key) {
                if !value.is_empty() {
                    return Language::from_locale(&value);
                }
            }
        }
        Language::English
    }

    /// `load_language` validator: enum name first, then locale lookup.
    pub fn parse(value: &str) -> Language {
        let upper = value.trim().to_uppercase();
        match upper.as_str() {
            "CHINESE" => Language::Chinese,
            "ENGLISH" => Language::English,
            "JAPANESE" => Language::Japanese,
            "GERMAN" => Language::German,
            _ => Language::from_locale(&upper),
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_ui_locale() -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
        fn LCIDToLocaleName(locale: u32, name: *mut u16, name_count: i32, flags: u32) -> i32;
    }
    // GetUserDefaultUILanguage returns a LANGID; turn it into an LCID understood
    // by LCIDToLocaleName (the low word is the language id).
    unsafe {
        let langid = GetUserDefaultUILanguage() as u32;
        let mut buffer = [0u16; 85];
        let written = LCIDToLocaleName(langid, buffer.as_mut_ptr(), buffer.len() as i32, 0);
        if written <= 0 {
            return None;
        }
        let name = String::from_utf16_lossy(&buffer[..(written - 1) as usize]);
        // `LCIDToLocaleName` yields e.g. "zh-CN"; `locale.windows_locale` uses
        // "zh_CN" and `from_locale` normalises both.
        Some(name.replace('-', "_"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DarkMode {
    Light,
    Dark,
    System,
}

impl DarkMode {
    pub fn name(self) -> &'static str {
        match self {
            DarkMode::Light => "LIGHT",
            DarkMode::Dark => "DARK",
            DarkMode::System => "SYSTEM",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_uppercase().as_str() {
            "LIGHT" => Some(DarkMode::Light),
            "DARK" => Some(DarkMode::Dark),
            "SYSTEM" => Some(DarkMode::System),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictPolicy {
    Skip,
    Overwrite,
    Prompt,
}

impl ConflictPolicy {
    pub fn name(self) -> &'static str {
        match self {
            ConflictPolicy::Skip => "SKIP",
            ConflictPolicy::Overwrite => "OVERWRITE",
            ConflictPolicy::Prompt => "PROMPT",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_uppercase().as_str() {
            "SKIP" => Some(ConflictPolicy::Skip),
            "OVERWRITE" => Some(ConflictPolicy::Overwrite),
            "PROMPT" => Some(ConflictPolicy::Prompt),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionMode {
    Direct,
    Split,
    Merge,
}

impl ConversionMode {
    pub fn name(self) -> &'static str {
        match self {
            ConversionMode::Direct => "DIRECT",
            ConversionMode::Split => "SPLIT",
            ConversionMode::Merge => "MERGE",
        }
    }
}

// ---------------------------------------------------------------------------
// Lyrics replacement rules
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LyricsReplaceMode {
    Full,
    Alphabetic,
    NonAlphabetic,
    Regex,
}

impl Default for LyricsReplaceMode {
    fn default() -> Self {
        Self::Full
    }
}

impl LyricsReplaceMode {
    pub fn name(self) -> &'static str {
        match self {
            LyricsReplaceMode::Full => "FULL",
            LyricsReplaceMode::Alphabetic => "ALPHABETIC",
            LyricsReplaceMode::NonAlphabetic => "NON_ALPHABETIC",
            LyricsReplaceMode::Regex => "REGEX",
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            LyricsReplaceMode::Full => "full",
            LyricsReplaceMode::Alphabetic => "alphabetic",
            LyricsReplaceMode::NonAlphabetic => "non_alphabetic",
            LyricsReplaceMode::Regex => "regex",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_uppercase().as_str() {
            "FULL" => Some(LyricsReplaceMode::Full),
            "ALPHABETIC" => Some(LyricsReplaceMode::Alphabetic),
            "NON_ALPHABETIC" => Some(LyricsReplaceMode::NonAlphabetic),
            "REGEX" => Some(LyricsReplaceMode::Regex),
            _ => match value.trim() {
                "full" => Some(LyricsReplaceMode::Full),
                "alphabetic" => Some(LyricsReplaceMode::Alphabetic),
                "non_alphabetic" => Some(LyricsReplaceMode::NonAlphabetic),
                "regex" => Some(LyricsReplaceMode::Regex),
                _ => None,
            },
        }
    }

    pub fn prefix_suffix(self) -> Option<(&'static str, &'static str)> {
        match self {
            LyricsReplaceMode::Full => Some(("^", "$")),
            LyricsReplaceMode::Alphabetic => Some((r"(?<=^|\b)", r"(?=$|\b)")),
            LyricsReplaceMode::NonAlphabetic => Some(("", "")),
            LyricsReplaceMode::Regex => None,
        }
    }
}

/// One lyric replacement rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LyricsReplacement {
    pub replacement: String,
    pub pattern_main: String,
    pub pattern_prefix: String,
    pub pattern_suffix: String,
    /// `re.RegexFlag` member name, e.g. `"IGNORECASE"`.
    pub flags: String,
    pub mode: LyricsReplaceMode,
}

impl LyricsReplacement {
    /// `__post_init__` — rewrites prefix/suffix from the mode and escapes the
    /// main pattern unless the mode is `REGEX`.
    pub fn normalize(&mut self) {
        if let Some((prefix, suffix)) = self.mode.prefix_suffix() {
            self.pattern_prefix = prefix.to_owned();
            self.pattern_suffix = suffix.to_owned();
        }
        if self.mode != LyricsReplaceMode::Regex {
            self.pattern_main = regex::escape(&self.pattern_main);
        }
    }

    /// The full pattern as Python's `_pattern` property builds it.
    pub fn pattern(&self) -> String {
        format!("{}{}{}", self.pattern_prefix, self.pattern_main, self.pattern_suffix)
    }

    /// `compiled_pattern`.
    pub fn compiled_pattern(&self) -> Result<regex::Regex> {
        let mut builder = regex::RegexBuilder::new(&self.pattern());
        for flag in self.flags.split('|').map(str::trim) {
            match flag {
                "IGNORECASE" | "I" => {
                    builder.case_insensitive(true);
                }
                "MULTILINE" | "M" => {
                    builder.multi_line(true);
                }
                "DOTALL" | "S" => {
                    builder.dot_matches_new_line(true);
                }
                "VERBOSE" | "X" => {
                    builder.ignore_whitespace(true);
                }
                "" | "UNICODE" | "U" | "ASCII" | "A" => {}
                other => {
                    return Err(LibreSvipError::msg(format!(
                        "unsupported regex flag: {other}"
                    )));
                }
            }
        }
        builder.build().map_err(LibreSvipError::from)
    }

    /// `replace`.
    pub fn replace(&self, text: &str) -> Result<String> {
        let pattern = self.compiled_pattern()?;
        Ok(pattern
            .replace_all(text, self.replacement.as_str())
            .into_owned())
    }
}

// ---------------------------------------------------------------------------
// Settings model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Settings {
    pub language: Language,
    pub last_input_format: Option<String>,
    pub last_output_format: Option<String>,
    pub dark_mode: DarkMode,
    pub auto_detect_input_format: bool,
    pub reset_tasks_on_input_change: bool,
    pub max_track_count: i64,
    pub lyric_replace_rules: IndexMap<String, Vec<LyricsReplacement>>,
    pub disabled_plugins: Vec<String>,
    // GUI-only fields, kept for round-tripping.
    pub save_folder: PathBuf,
    pub folder_presets: Vec<PathBuf>,
    pub conflict_policy: ConflictPolicy,
    pub multi_threaded_conversion: bool,
    pub open_save_folder_on_completion: bool,
    pub auto_set_output_extension: bool,
    pub auto_check_for_updates: bool,
    /// `extra="allow"` extras, preserved verbatim.
    pub extra: IndexMap<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut lyric_replace_rules = IndexMap::new();
        lyric_replace_rules.insert("default".to_owned(), Vec::new());
        Self {
            language: Language::auto(),
            last_input_format: None,
            last_output_format: None,
            dark_mode: DarkMode::System,
            auto_detect_input_format: true,
            reset_tasks_on_input_change: true,
            max_track_count: 1,
            lyric_replace_rules,
            disabled_plugins: Vec::new(),
            save_folder: PathBuf::from("."),
            folder_presets: Vec::new(),
            conflict_policy: ConflictPolicy::Prompt,
            multi_threaded_conversion: true,
            open_save_folder_on_completion: true,
            auto_set_output_extension: true,
            auto_check_for_updates: true,
            extra: IndexMap::new(),
        }
    }
}

impl Settings {
    /// `lyric_replace_rules_groups`.
    pub fn lyric_replace_rules_groups(&self) -> Vec<String> {
        self.lyric_replace_rules.keys().cloned().collect()
    }

    /// `conf list` order — exactly the Python field declaration order.
    pub fn dump_fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("language", self.language.name().to_owned()),
            (
                "last_input_format",
                match &self.last_input_format {
                    Some(value) => value.clone(),
                    None => "None".to_owned(),
                },
            ),
            (
                "last_output_format",
                match &self.last_output_format {
                    Some(value) => value.clone(),
                    None => "None".to_owned(),
                },
            ),
            ("dark_mode", self.dark_mode.name().to_owned()),
            (
                "auto_detect_input_format",
                python_bool(self.auto_detect_input_format),
            ),
            (
                "reset_tasks_on_input_change",
                python_bool(self.reset_tasks_on_input_change),
            ),
            ("max_track_count", self.max_track_count.to_string()),
            (
                "lyric_replace_rules",
                python_repr_rules(&self.lyric_replace_rules),
            ),
            (
                "disabled_plugins",
                python_repr_list(&self.disabled_plugins),
            ),
            ("save_folder", python_repr_path(&self.save_folder)),
            (
                "folder_presets",
                python_repr_paths(&self.folder_presets),
            ),
            ("conflict_policy", self.conflict_policy.name().to_owned()),
            (
                "multi_threaded_conversion",
                python_bool(self.multi_threaded_conversion),
            ),
            (
                "open_save_folder_on_completion",
                python_bool(self.open_save_folder_on_completion),
            ),
            (
                "auto_set_output_extension",
                python_bool(self.auto_set_output_extension),
            ),
            (
                "auto_check_for_updates",
                python_bool(self.auto_check_for_updates),
            ),
        ]
    }

    /// Validates `last_*_format` against the disabled plugin list.
    fn validate_last_formats(&mut self) {
        if let Some(value) = &self.last_input_format {
            if self.disabled_plugins.iter().any(|item| item == value) {
                self.last_input_format = None;
            }
        }
        if let Some(value) = &self.last_output_format {
            if self.disabled_plugins.iter().any(|item| item == value) {
                self.last_output_format = None;
            }
        }
    }
}

fn python_bool(value: bool) -> String {
    if value {
        "True".to_owned()
    } else {
        "False".to_owned()
    }
}

fn python_repr_list(values: &[String]) -> String {
    let inner = values
        .iter()
        .map(|value| format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

fn python_repr_path(path: &std::path::Path) -> String {
    path.display().to_string()
}

fn python_repr_paths(paths: &[PathBuf]) -> String {
    let inner = paths
        .iter()
        .map(|path| format!("WindowsPath('{}')", path.display().to_string().replace('\\', "/")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

fn python_repr_rules(rules: &IndexMap<String, Vec<LyricsReplacement>>) -> String {
    let inner = rules
        .iter()
        .map(|(group, items)| {
            let rules = items
                .iter()
                .map(|rule| {
                    format!(
                        "{{'replacement': '{}', 'pattern_main': '{}', 'pattern_prefix': '{}', \
                         'pattern_suffix': '{}', 'flags': '{}', 'mode': '{}'}}",
                        rule.replacement.replace('\'', "\\'"),
                        rule.pattern_main.replace('\'', "\\'"),
                        rule.pattern_prefix.replace('\'', "\\'"),
                        rule.pattern_suffix.replace('\'', "\\'"),
                        rule.flags,
                        rule.mode.name(),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("'{group}': [{rules}]")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{inner}}}")
}

// ---------------------------------------------------------------------------
// YAML loading
// ---------------------------------------------------------------------------

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn value_to_bool(value: &Value, default: bool) -> bool {
    match value {
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_i64().map(|n| n != 0).unwrap_or(default),
        Value::String(text) => matches!(text.as_str(), "true" | "True" | "1" | "yes" | "on"),
        _ => default,
    }
}

fn value_to_i64(value: &Value, default: i64) -> i64 {
    match value {
        Value::Number(number) => number.as_i64().unwrap_or(default),
        Value::String(text) => text.trim().parse().unwrap_or(default),
        _ => default,
    }
}

/// Class name PyYAML uses for `pathlib.Path` objects on this platform.
pub fn python_path_class() -> &'static str {
    if cfg!(windows) {
        "python/object/apply:pathlib.WindowsPath"
    } else {
        "python/object/apply:pathlib.PosixPath"
    }
}

/// `tuple(PurePath(path)._parts)`: `Path("./")` has no parts, `C:\\a` becomes
/// `["C:\\", "a"]`.
pub fn python_path_parts(path: &std::path::Path) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut prefix = String::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(value) => {
                prefix = value.as_os_str().to_string_lossy().into_owned();
            }
            std::path::Component::RootDir => {
                prefix.push(std::path::MAIN_SEPARATOR);
            }
            std::path::Component::CurDir => {}
            std::path::Component::Normal(value) => {
                if !prefix.is_empty() {
                    parts.push(std::mem::take(&mut prefix));
                }
                parts.push(value.to_string_lossy().into_owned());
            }
            std::path::Component::ParentDir => {
                if !prefix.is_empty() {
                    parts.push(std::mem::take(&mut prefix));
                }
                parts.push("..".to_owned());
            }
        }
    }
    if !prefix.is_empty() {
        parts.insert(0, prefix);
    }
    parts
}

fn value_to_path(value: &Value) -> Option<PathBuf> {
    match value {
        Value::String(text) => Some(PathBuf::from(text)),
        Value::Tagged(tagged) => match &tagged.value {
            Value::Sequence(items) => {
                let parts: Vec<String> = items.iter().filter_map(value_to_string).collect();
                let mut path = PathBuf::new();
                for part in parts {
                    if part == "." || part.is_empty() {
                        continue;
                    }
                    path.push(part);
                }
                if path.as_os_str().is_empty() {
                    path.push(".");
                }
                Some(path)
            }
            Value::String(text) => Some(PathBuf::from(text)),
            _ => None,
        },
        Value::Sequence(items) => {
            let parts: Vec<String> = items.iter().filter_map(value_to_string).collect();
            Some(PathBuf::from(parts.join(std::path::MAIN_SEPARATOR_STR)))
        }
        _ => None,
    }
}

fn value_to_rules(value: &Value) -> Option<IndexMap<String, Vec<LyricsReplacement>>> {
    let Value::Mapping(mapping) = value else {
        return None;
    };
    let mut rules = IndexMap::new();
    for (key, group) in mapping {
        let group_name = value_to_string(key)?;
        let mut items = Vec::new();
        if let Value::Sequence(entries) = group {
            for entry in entries {
                let Value::Mapping(fields) = entry else {
                    continue;
                };
                let mut rule = LyricsReplacement {
                    flags: "IGNORECASE".to_owned(),
                    mode: LyricsReplaceMode::Full,
                    ..Default::default()
                };
                for (field_key, field_value) in fields {
                    let Some(name) = value_to_string(field_key) else {
                        continue;
                    };
                    let text = value_to_string(field_value).unwrap_or_default();
                    match name.as_str() {
                        "replacement" => rule.replacement = text,
                        "pattern_main" => rule.pattern_main = text,
                        "pattern_prefix" => rule.pattern_prefix = text,
                        "pattern_suffix" => rule.pattern_suffix = text,
                        "flags" => rule.flags = text,
                        "mode" => {
                            rule.mode = LyricsReplaceMode::parse(&text).unwrap_or(LyricsReplaceMode::Full);
                        }
                        _ => {}
                    }
                }
                rule.normalize();
                items.push(rule);
            }
        }
        rules.insert(group_name, items);
    }
    Some(rules)
}

impl Settings {
    fn from_value(root: &Value) -> Result<Self> {
        let Value::Mapping(mapping) = root else {
            return Err(LibreSvipError::Validation(
                "settings.yml must contain a mapping".to_owned(),
            ));
        };
        let mut settings = Settings::default();
        settings.extra.clear();
        let mut seen: Vec<String> = Vec::new();
        for (key, value) in mapping {
            let Some(name) = value_to_string(key) else {
                continue;
            };
            seen.push(name.clone());
            match name.as_str() {
                "language" => {
                    if let Some(text) = value_to_string(value) {
                        settings.language = Language::parse(&text);
                    }
                }
                "last_input_format" => {
                    settings.last_input_format = value_to_string(value);
                }
                "last_output_format" => {
                    settings.last_output_format = value_to_string(value);
                }
                "dark_mode" => {
                    let text = value_to_string(value).unwrap_or_default();
                    settings.dark_mode = DarkMode::parse(&text).ok_or_else(|| {
                        LibreSvipError::Validation(format!("invalid dark_mode: {text}"))
                    })?;
                }
                "auto_detect_input_format" => {
                    settings.auto_detect_input_format = value_to_bool(value, true);
                }
                "reset_tasks_on_input_change" => {
                    settings.reset_tasks_on_input_change = value_to_bool(value, true);
                }
                "max_track_count" => {
                    settings.max_track_count = value_to_i64(value, 1);
                }
                "lyric_replace_rules" => {
                    settings.lyric_replace_rules =
                        value_to_rules(value).unwrap_or_default();
                }
                "disabled_plugins" => {
                    settings.disabled_plugins = match value {
                        Value::Sequence(items) => {
                            items.iter().filter_map(value_to_string).collect()
                        }
                        _ => Vec::new(),
                    };
                }
                "save_folder" => {
                    settings.save_folder =
                        value_to_path(value).unwrap_or_else(|| PathBuf::from("."));
                }
                "folder_presets" => {
                    settings.folder_presets = match value {
                        Value::Sequence(items) => {
                            items.iter().filter_map(value_to_path).collect()
                        }
                        _ => Vec::new(),
                    };
                }
                "conflict_policy" => {
                    let text = value_to_string(value).unwrap_or_default();
                    settings.conflict_policy = ConflictPolicy::parse(&text).ok_or_else(|| {
                        LibreSvipError::Validation(format!("invalid conflict_policy: {text}"))
                    })?;
                }
                "multi_threaded_conversion" => {
                    settings.multi_threaded_conversion = value_to_bool(value, true);
                }
                "open_save_folder_on_completion" => {
                    settings.open_save_folder_on_completion = value_to_bool(value, true);
                }
                "auto_set_output_extension" => {
                    settings.auto_set_output_extension = value_to_bool(value, true);
                }
                "auto_check_for_updates" => {
                    settings.auto_check_for_updates = value_to_bool(value, true);
                }
                other => {
                    settings
                        .extra
                        .insert(other.to_owned(), value.clone());
                }
            }
        }
        let _ = seen;
        if !settings.lyric_replace_rules.contains_key("default") {
            settings
                .lyric_replace_rules
                .insert("default".to_owned(), Vec::new());
        }
        settings.validate_last_formats();
        Ok(settings)
    }
}

// ---------------------------------------------------------------------------
// YAML emission (PyYAML compatible subset)
// ---------------------------------------------------------------------------

enum YamlOut {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<YamlOut>),
    Map(Vec<(String, YamlOut)>),
    Tagged(String, Vec<YamlOut>),
}

fn is_yaml_bool(text: &str) -> bool {
    matches!(
        text,
        "y" | "Y"
            | "yes"
            | "Yes"
            | "YES"
            | "n"
            | "N"
            | "no"
            | "No"
            | "NO"
            | "true"
            | "True"
            | "TRUE"
            | "false"
            | "False"
            | "FALSE"
            | "on"
            | "On"
            | "ON"
            | "off"
            | "Off"
            | "OFF"
    )
}

fn emit_scalar(value: &YamlOut) -> String {
    match value {
        YamlOut::Bool(flag) => {
            if *flag {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        }
        YamlOut::Int(number) => number.to_string(),
        YamlOut::Float(number) => crate::options::format_float(*number),
        YamlOut::Str(text) => {
            let needs_quote = text.is_empty()
                || is_yaml_bool(text)
                || text.parse::<i64>().is_ok()
                || text.parse::<f64>().is_ok()
                || text.starts_with(['!', '&', '*', '?', '|', '>', '%', '@', '`', '"', '\''])
                || text.starts_with(' ')
                || text.ends_with(' ')
                || text.contains(": ")
                || text.contains(" #")
                || text.contains('\n')
                || text.contains('\t');
            if needs_quote {
                format!("'{}'", text.replace('\'', "''"))
            } else {
                text.clone()
            }
        }
        _ => String::new(),
    }
}

fn is_scalar(value: &YamlOut) -> bool {
    !matches!(value, YamlOut::List(_) | YamlOut::Map(_) | YamlOut::Tagged(_, _))
}

fn emit_list_items(items: &[YamlOut], indent: usize, out: &mut String) {
    let padding = " ".repeat(indent);
    for item in items {
        out.push_str(&padding);
        out.push('-');
        match item {
            YamlOut::Map(entries) if !entries.is_empty() => {
                out.push(' ');
                emit_map_entries(entries, indent + 2, out, true);
            }
            YamlOut::List(inner) => {
                out.push('\n');
                emit_list_items(inner, indent + 2, out);
            }
            scalar => {
                out.push(' ');
                out.push_str(&emit_scalar(scalar));
                out.push('\n');
            }
        }
    }
}

fn emit_map_entries(entries: &[(String, YamlOut)], indent: usize, out: &mut String, inline_first: bool) {
    let padding = " ".repeat(indent);
    for (index, (key, value)) in entries.iter().enumerate() {
        if !(inline_first && index == 0) {
            out.push_str(&padding);
        }
        out.push_str(key);
        out.push(':');
        match value {
            YamlOut::Map(inner) if inner.is_empty() => {
                out.push_str(" {}\n");
            }
            YamlOut::Map(inner) => {
                out.push('\n');
                emit_map_entries(inner, indent + 2, out, false);
            }
            YamlOut::List(inner) => {
                if inner.is_empty() {
                    out.push_str(" []\n");
                } else if inner.iter().all(is_scalar) {
                    out.push('\n');
                    emit_list_items(inner, indent, out);
                } else {
                    out.push('\n');
                    emit_list_items(inner, indent, out);
                }
            }
            YamlOut::Tagged(tag, args) => {
                out.push(' ');
                out.push_str("!!");
                out.push_str(tag);
                if args.is_empty() {
                    // PyYAML renders an empty tagged sequence inline: `... []`
                    out.push_str(" []\n");
                } else {
                    out.push('\n');
                    emit_list_items(args, indent, out);
                }
            }
            scalar => {
                out.push(' ');
                out.push_str(&emit_scalar(scalar));
                out.push('\n');
            }
        }
    }
}

fn settings_to_yaml(settings: &Settings) -> String {
    let mut entries: Vec<(String, YamlOut)> = vec![
        (
            "auto_check_for_updates".to_owned(),
            YamlOut::Bool(settings.auto_check_for_updates),
        ),
        (
            "auto_detect_input_format".to_owned(),
            YamlOut::Bool(settings.auto_detect_input_format),
        ),
        (
            "auto_set_output_extension".to_owned(),
            YamlOut::Bool(settings.auto_set_output_extension),
        ),
        (
            "conflict_policy".to_owned(),
            YamlOut::Str(settings.conflict_policy.name().to_owned()),
        ),
        (
            "dark_mode".to_owned(),
            YamlOut::Str(settings.dark_mode.name().to_owned()),
        ),
        (
            "disabled_plugins".to_owned(),
            YamlOut::List(
                settings
                    .disabled_plugins
                    .iter()
                    .cloned()
                    .map(YamlOut::Str)
                    .collect(),
            ),
        ),
        (
            "folder_presets".to_owned(),
            YamlOut::List(
                settings
                    .folder_presets
                    .iter()
                    .map(|path| {
                        YamlOut::Tagged(
                            python_path_class().to_owned(),
                            python_path_parts(path)
                                .into_iter()
                                .map(YamlOut::Str)
                                .collect(),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "language".to_owned(),
            YamlOut::Str(settings.language.name().to_owned()),
        ),
        (
            "last_input_format".to_owned(),
            match &settings.last_input_format {
                Some(value) => YamlOut::Str(value.clone()),
                None => YamlOut::Str("null".to_owned()),
            },
        ),
        (
            "last_output_format".to_owned(),
            match &settings.last_output_format {
                Some(value) => YamlOut::Str(value.clone()),
                None => YamlOut::Str("null".to_owned()),
            },
        ),
        (
            "lyric_replace_rules".to_owned(),
            YamlOut::Map(
                settings
                    .lyric_replace_rules
                    .iter()
                    .map(|(group, rules)| {
                        (
                            group.clone(),
                            YamlOut::List(
                                rules
                                    .iter()
                                    .map(|rule| {
                                        YamlOut::Map(vec![
                                            (
                                                "replacement".to_owned(),
                                                YamlOut::Str(rule.replacement.clone()),
                                            ),
                                            (
                                                "pattern_main".to_owned(),
                                                YamlOut::Str(rule.pattern_main.clone()),
                                            ),
                                            (
                                                "pattern_prefix".to_owned(),
                                                YamlOut::Str(rule.pattern_prefix.clone()),
                                            ),
                                            (
                                                "pattern_suffix".to_owned(),
                                                YamlOut::Str(rule.pattern_suffix.clone()),
                                            ),
                                            (
                                                "flags".to_owned(),
                                                YamlOut::Str(rule.flags.clone()),
                                            ),
                                            (
                                                "mode".to_owned(),
                                                YamlOut::Str(rule.mode.name().to_owned()),
                                            ),
                                        ])
                                    })
                                    .collect(),
                            ),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "max_track_count".to_owned(),
            YamlOut::Int(settings.max_track_count),
        ),
        (
            "multi_threaded_conversion".to_owned(),
            YamlOut::Bool(settings.multi_threaded_conversion),
        ),
        (
            "open_save_folder_on_completion".to_owned(),
            YamlOut::Bool(settings.open_save_folder_on_completion),
        ),
        (
            "reset_tasks_on_input_change".to_owned(),
            YamlOut::Bool(settings.reset_tasks_on_input_change),
        ),
        (
            "save_folder".to_owned(),
            YamlOut::Tagged(
                python_path_class().to_owned(),
                python_path_parts(&settings.save_folder)
                    .into_iter()
                    .map(YamlOut::Str)
                    .collect(),
            ),
        ),
    ];
    for (key, value) in &settings.extra {
        entries.push((key.clone(), yamlout_from_value(value)));
    }
    // PyYAML sorts mapping keys by default (`sort_keys=True`).
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    emit_map_entries(&entries, 0, &mut out, false);
    out
}

fn yamlout_from_value(value: &Value) -> YamlOut {
    match value {
        Value::Null => YamlOut::Str("null".to_owned()),
        Value::Bool(flag) => YamlOut::Bool(*flag),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                YamlOut::Int(int)
            } else {
                YamlOut::Float(number.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(text) => YamlOut::Str(text.clone()),
        Value::Sequence(items) => YamlOut::List(items.iter().map(yamlout_from_value).collect()),
        Value::Mapping(mapping) => YamlOut::Map(
            mapping
                .iter()
                .map(|(key, value)| {
                    (
                        value_to_string(key).unwrap_or_default(),
                        yamlout_from_value(value),
                    )
                })
                .collect(),
        ),
        Value::Tagged(tagged) => YamlOut::Tagged(
            tagged.tag.to_string(),
            match &tagged.value {
                Value::Sequence(items) => items.iter().map(yamlout_from_value).collect(),
                other => vec![yamlout_from_value(other)],
            },
        ),
    }
}

// ---------------------------------------------------------------------------
// Global settings handle
// ---------------------------------------------------------------------------

fn state() -> &'static RwLock<Settings> {
    static STATE: OnceLock<RwLock<Settings>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(load_settings()))
}

/// Path of `settings.yml`.
pub fn config_path() -> PathBuf {
    app_dirs().user_config_path.join(SETTINGS_FILENAME)
}

fn load_settings() -> Settings {
    let path = config_path();
    if !path.exists() {
        let mut settings = Settings::default();
        settings.validate_last_formats();
        return settings;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return Settings::default(),
    };
    match serde_yaml::from_str::<Value>(&text) {
        Ok(value) => match Settings::from_value(&value) {
            Ok(settings) => settings,
            Err(_) => {
                // `raise_error_if_failed=False` falls back to defaults.
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    }
}

/// Snapshot of the current settings.
pub fn settings() -> Settings {
    state().read().unwrap().clone()
}

/// Reads a value from the live settings object.
pub fn read_settings<T>(f: impl FnOnce(&Settings) -> T) -> T {
    f(&state().read().unwrap())
}

/// Mutates the live settings object without saving.
pub fn update_settings<T>(f: impl FnOnce(&mut Settings) -> T) -> T {
    f(&mut state().write().unwrap())
}

/// `save_settings()`.
pub fn save_settings() -> Result<()> {
    let snapshot = settings();
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).at_path(parent)?;
    }
    std::fs::write(&path, settings_to_yaml(&snapshot)).at_path(&path)?;
    Ok(())
}

/// `conf set language`.
pub fn set_language(value: &str) -> Result<()> {
    let language = Language::parse(value);
    update_settings(|settings| settings.language = language);
    save_settings()
}

/// Guard for tests that need to swap the settings file location.
pub fn force_reload() {
    let mut guard = state().write().unwrap();
    *guard = load_settings();
}

/// Ensures `cache_dir` exists, mirroring `app_dir.user_cache_path`.
pub fn ensure_cache_dir() -> Result<PathBuf> {
    let dir = app_dirs().user_cache_path.clone();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap();
    std::fs::create_dir_all(&dir).at_path(&dir)?;
    Ok(dir)
}
