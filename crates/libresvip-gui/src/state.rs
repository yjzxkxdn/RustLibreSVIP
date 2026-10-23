//! GUI state: format entries, option field models and task rows.

use std::path::{Path, PathBuf};

use libresvip_core::options::{OptionFieldDef, OptionKind, OptionValue};
use libresvip_core::plugin::SvsPlugin;
use libresvip_plugins::{all_plugins, enabled_plugins};

/// One entry of the input/output format combo boxes.
#[derive(Clone, Debug)]
pub struct FormatEntry {
    pub id: &'static str,
    pub file_format: &'static str,
    pub suffix: &'static str,
    pub suffixes: &'static [&'static str],
    pub name: &'static str,
    pub author: &'static str,
    pub website: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub readable: bool,
    pub writable: bool,
}

impl FormatEntry {
    fn new(plugin: &'static dyn SvsPlugin) -> Self {
        let info = plugin.info();
        Self {
            id: info.id,
            file_format: info.file_format,
            suffix: info.suffix(),
            suffixes: info.suffixes,
            name: info.name,
            author: info.author,
            website: info.website,
            description: info.description,
            version: plugin.version(),
            readable: plugin.readable(),
            writable: plugin.writable(),
        }
    }

    /// `"{file_format} (*.{suffix})"` — the text Qt shows in the combo box.
    #[allow(dead_code)]
    pub fn display(&self) -> String {        format!("{} (*.{})", self.file_format, self.suffix)
    }

    /// `"*.a; *.b"` — the dialog filter description.
    pub fn suffixes_display(&self) -> String {
        self.suffixes
            .iter()
            .map(|suffix| format!("*.{suffix}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Input formats: everything that can be read (WriteOnly plugins excluded).
pub fn input_formats() -> Vec<FormatEntry> {
    enabled_plugins()
        .into_iter()
        .filter(|plugin| plugin.readable())
        .map(FormatEntry::new)
        .collect()
}

/// Output formats: everything that can be written (ReadOnly plugins excluded).
pub fn output_formats() -> Vec<FormatEntry> {
    enabled_plugins()
        .into_iter()
        .filter(|plugin| plugin.writable())
        .map(FormatEntry::new)
        .collect()
}

/// Every registered plugin, for the plugin list in the settings dialog.
pub fn all_format_entries() -> Vec<FormatEntry> {
    all_plugins().iter().copied().map(FormatEntry::new).collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldKind {
    Bool,
    Int,
    Float,
    Text,
    Enum,
}

/// Editable state of one conversion option field.
#[derive(Clone, Debug)]
pub struct FieldState {
    pub key: &'static str,
    pub title: &'static str,
    pub description: Option<&'static str>,
    pub kind: FieldKind,
    pub text: String,
    pub bool_value: bool,
    pub enum_index: usize,
    /// `(value, title)` pairs for enum fields.
    pub choices: Vec<(&'static str, &'static str)>,
    pub default_text: String,
}

impl FieldState {
    pub fn from_def(def: &OptionFieldDef) -> Self {
        let default_text = match &def.default {
            OptionValue::Bool(value) => {
                if *value {
                    "True".to_owned()
                } else {
                    "False".to_owned()
                }
            }
            OptionValue::Int(value) => value.to_string(),
            OptionValue::Float(value) => def
                .default_repr
                .clone()
                .unwrap_or_else(|| value.to_string()),
            OptionValue::Str(value) => value.clone(),
        };
        let mut state = Self {
            key: def.key,
            title: def.title,
            description: def.description,
            kind: FieldKind::Text,
            text: default_text.clone(),
            bool_value: matches!(&def.default, OptionValue::Bool(true)),
            enum_index: 0,
            choices: Vec::new(),
            default_text,
        };
        match &def.kind {
            OptionKind::Bool => state.kind = FieldKind::Bool,
            OptionKind::Int => state.kind = FieldKind::Int,
            OptionKind::Float => state.kind = FieldKind::Float,
            OptionKind::Enum { variants, .. } => {
                state.kind = FieldKind::Enum;
                state.choices = variants
                    .iter()
                    .map(|variant| (variant.value, variant.title))
                    .collect();
                let default_value = match &def.default {
                    OptionValue::Str(value) => value.clone(),
                    other => other.to_display_string(),
                };
                state.enum_index = state
                    .choices
                    .iter()
                    .position(|(value, _)| *value == default_value)
                    .unwrap_or(0);
                if let Some((value, _)) = state.choices.get(state.enum_index) {
                    state.text = (*value).to_owned();
                }
            }
            OptionKind::Opaque { .. } | OptionKind::Complex { .. } => {
                state.kind = FieldKind::Text;
            }
            OptionKind::Str => {
                if let Some(choices) = def.choices {
                    state.kind = FieldKind::Enum;
                    state.choices = choices.iter().map(|choice| (*choice, *choice)).collect();
                    let default = state.text.clone();
                    state.enum_index = state
                        .choices
                        .iter()
                        .position(|(value, _)| *value == default)
                        .unwrap_or(0);
                }
            }
        }
        state
    }

    #[allow(dead_code)]
    pub fn is_default(&self) -> bool {
        match self.kind {
            FieldKind::Bool => self.bool_value == (self.default_text == "True"),
            FieldKind::Enum => self.text == self.default_text,
            _ => self.text == self.default_text,
        }
    }

    /// Value passed to `load`/`dump`.
    pub fn to_value(&self) -> OptionValue {
        match self.kind {
            FieldKind::Bool => OptionValue::Bool(self.bool_value),
            FieldKind::Int => {
                let fallback = self.default_text.trim().parse::<i64>().unwrap_or(0);
                OptionValue::Int(self.text.trim().parse().unwrap_or(fallback))
            }
            FieldKind::Float => {
                let fallback = self.default_text.trim().parse::<f64>().unwrap_or(0.0);
                OptionValue::Float(self.text.trim().parse().unwrap_or(fallback))
            }
            FieldKind::Enum => OptionValue::Str(
                self.choices
                    .get(self.enum_index)
                    .map(|(value, _)| (*value).to_owned())
                    .unwrap_or_else(|| self.text.clone()),
            ),
            FieldKind::Text => OptionValue::Str(self.text.clone()),
        }
    }
}

/// One middleware section in the advanced settings panel.
#[derive(Clone, Debug)]
pub struct MiddlewareState {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub enabled: bool,
    pub fields: Vec<FieldState>,
}

/// One row of the conversion task list.
#[derive(Clone, Debug)]
pub struct Task {
    pub path: PathBuf,
    pub name: String,
    pub stem: String,
    /// Output extension including the leading dot, e.g. `.acep`.
    pub ext: String,
    pub running: bool,
    pub success: Option<bool>,
    pub skipped: bool,
    pub error: String,
    pub warning: String,
    pub tmp: Option<PathBuf>,
}

impl Task {
    pub fn new(path: PathBuf, ext: String) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.clone());
        Self {
            path,
            name,
            stem,
            ext,
            running: false,
            success: None,
            skipped: false,
            error: String::new(),
            warning: String::new(),
            tmp: None,
        }
    }

    pub fn reset_status(&mut self) {
        self.running = false;
        self.success = None;
        self.skipped = false;
        self.error.clear();
        self.warning.clear();
        self.tmp = None;
    }
}

/// `ConfigItems.dir_valid`: relative paths are always accepted, absolute paths
/// must exist and be directories.
pub fn dir_valid(value: &str) -> bool {
    let path = Path::new(value);
    (!path.is_absolute() || path.exists()) && path.is_dir()
}

/// `TaskManager.output_dir`.
pub fn output_dir(save_folder: &Path, task: &Task) -> PathBuf {
    if save_folder.is_absolute() {
        save_folder.to_path_buf()
    } else {
        task.path
            .parent()
            .map(|parent| parent.join(save_folder))
            .unwrap_or_else(|| save_folder.to_path_buf())
    }
}

/// `f"{stem}{ext}"` — the file name the task produces.
pub fn output_file_name(stem: &str, ext: &str) -> String {
    format!("{stem}{ext}")
}
