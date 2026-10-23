//! Schema-driven conversion options.
//!
//! The Python implementation introspects pydantic models (`model_fields`) to
//! build both the interactive prompts and `plugin detail` output. Rust has no
//! runtime reflection, so every plugin returns an explicit [`OptionFieldDef`]
//! list instead. The shape of that list is the crate-level contract: keys,
//! titles, defaults and order must match the Python option classes exactly.

use indexmap::IndexMap;

/// A concrete option value supplied by the CLI.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl OptionValue {
    /// Renders the value the way Python's `str()` would for prompt defaults.
    pub fn to_display_string(&self) -> String {
        match self {
            OptionValue::Bool(value) => {
                if *value {
                    "True".to_owned()
                } else {
                    "False".to_owned()
                }
            }
            OptionValue::Int(value) => value.to_string(),
            OptionValue::Float(value) => format_float(*value),
            OptionValue::Str(value) => value.clone(),
        }
    }
}

/// Formats a float like Python's `repr`, which keeps the `.0` suffix.
pub fn format_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        let mut rendered = value.to_string();
        if !rendered.contains('.') && !rendered.contains('e') && !rendered.contains('E') {
            rendered.push_str(".0");
        }
        rendered
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnumVariant {
    /// Member name (`enum` name), e.g. `"AUTO"`.
    pub name: &'static str,
    /// Value written to the format file, e.g. `"auto"`.
    pub value: &'static str,
    /// Translated title shown by `plugin detail`, e.g. `"Full match"`.
    pub title: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComplexFieldDef {
    pub name: &'static str,
    pub type_name: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OptionKind {
    Bool,
    Int,
    Float,
    Str,
    /// `enum.Enum` subclass: values accepted on the command line.
    Enum {
        type_name: &'static str,
        variants: &'static [EnumVariant],
    },
    /// `BaseComplexModel` subclass: edited through a string representation.
    Complex {
        type_name: &'static str,
        fields: &'static [ComplexFieldDef],
    },
    /// A type the Python `plugin detail` command skips (e.g. `Color` from
    /// pydantic-extra-types); the CLI still prompts for it as a string.
    Opaque { type_name: &'static str },
}

impl OptionKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            OptionKind::Bool => "bool",
            OptionKind::Int => "int",
            OptionKind::Float => "float",
            OptionKind::Str => "str",
            OptionKind::Enum { type_name, .. } => type_name,
            OptionKind::Complex { type_name, .. } => type_name,
            OptionKind::Opaque { type_name } => type_name,
        }
    }
}

/// One option field, mirroring one entry of `model_fields`.
#[derive(Clone, Debug)]
pub struct OptionFieldDef {
    pub key: &'static str,
    pub title: &'static str,
    /// `None` when the Python field has no description (printed as `None`).
    pub description: Option<&'static str>,
    pub kind: OptionKind,
    pub default: OptionValue,
    /// Overrides the `plugin detail` default rendering (enum members print as
    /// `ClassName.MEMBER`).
    pub default_repr: Option<String>,
    /// `json_schema_extra={"enum": ...}` choices (encoding lists, preset names).
    pub choices: Option<&'static [&'static str]>,
    /// Whether `plugin detail` prints the "Available values:" block. Python only
    /// does this for enums whose `_value_` annotation carries a pydantic model
    /// with member titles; plain `StrEnum`s are printed without it.
    pub show_enum_values: bool,
}

impl OptionFieldDef {
    pub const fn bool(key: &'static str, title: &'static str, default: bool) -> Self {
        Self {
            key,
            title,
            description: None,
            kind: OptionKind::Bool,
            default: OptionValue::Bool(default),
            default_repr: None,
            choices: None,
            show_enum_values: true,
        }
    }

    pub const fn int(key: &'static str, title: &'static str, default: i64) -> Self {
        Self {
            key,
            title,
            description: None,
            kind: OptionKind::Int,
            default: OptionValue::Int(default),
            default_repr: None,
            choices: None,
            show_enum_values: true,
        }
    }

    pub const fn float(key: &'static str, title: &'static str, default: f64) -> Self {
        Self {
            key,
            title,
            description: None,
            kind: OptionKind::Float,
            default: OptionValue::Float(default),
            default_repr: None,
            choices: None,
            show_enum_values: true,
        }
    }

    pub fn string(key: &'static str, title: &'static str, default: &'static str) -> Self {
        Self {
            key,
            title,
            description: None,
            kind: OptionKind::Str,
            default: OptionValue::Str(default.to_owned()),
            default_repr: None,
            choices: None,
            show_enum_values: true,
        }
    }

    /// `default_name` is the enum member name, e.g. `"AUTO"`; its value is
    /// looked up in `variants`.
    pub fn enumerated(
        key: &'static str,
        title: &'static str,
        type_name: &'static str,
        variants: &'static [EnumVariant],
        default_name: &'static str,
    ) -> Self {
        let default_value = variants
            .iter()
            .find(|variant| variant.name == default_name)
            .map(|variant| variant.value)
            .unwrap_or(default_name);
        Self {
            key,
            title,
            description: None,
            kind: OptionKind::Enum {
                type_name,
                variants,
            },
            default: OptionValue::Str(default_value.to_owned()),
            default_repr: Some(format!("{type_name}.{default_name}")),
            choices: None,
            show_enum_values: true,
        }
    }

    /// An enum without a `_value_` model: still prompted with its member values,
    /// but `plugin detail` does not print an "Available values:" block.
    pub fn enumerated_plain(
        key: &'static str,
        title: &'static str,
        type_name: &'static str,
        variants: &'static [EnumVariant],
        default: &'static str,
    ) -> Self {
        let mut field = Self::enumerated(key, title, type_name, variants, default);
        field.show_enum_values = false;
        field
    }

    /// A field of a type `plugin detail` does not describe (e.g. `Color`).
    pub fn opaque(
        key: &'static str,
        title: &'static str,
        default: &'static str,
        type_name: &'static str,
    ) -> Self {
        Self {
            key,
            title,
            description: None,
            kind: OptionKind::Opaque { type_name },
            default: OptionValue::Str(default.to_owned()),
            default_repr: None,
            choices: None,
            show_enum_values: true,
        }
    }

    pub fn with_description(mut self, description: &'static str) -> Self {
        self.description = Some(description);
        self
    }

    pub const fn with_choices(mut self, choices: &'static [&'static str]) -> Self {
        self.choices = Some(choices);
        self
    }

    /// The value type name used by `plugin detail`.
    pub fn type_name(&self) -> &'static str {
        self.kind.type_name()
    }

    /// Values accepted by an interactive prompt for this field.
    pub fn prompt_choices(&self) -> Option<Vec<&'static str>> {
        match &self.kind {
            OptionKind::Enum { variants, .. } => {
                Some(variants.iter().map(|variant| variant.value).collect())
            }
            _ => self.choices.map(|choices| choices.to_vec()),
        }
    }
}

/// The option dictionary handed to `load`/`dump`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options(IndexMap<String, OptionValue>);

impl Options {
    pub fn new() -> Self {
        Self(IndexMap::new())
    }

    pub fn from_pairs<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (String, OptionValue)>,
    {
        Self(pairs.into_iter().collect())
    }

    pub fn insert(&mut self, key: impl Into<String>, value: OptionValue) -> Option<OptionValue> {
        self.0.insert(key.into(), value)
    }

    pub fn get(&self, key: &str) -> Option<&OptionValue> {
        self.0.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &OptionValue)> {
        self.0.iter()
    }

    pub fn bool(&self, key: &str, default: bool) -> bool {
        match self.0.get(key) {
            Some(OptionValue::Bool(value)) => *value,
            Some(OptionValue::Int(value)) => *value != 0,
            Some(OptionValue::Float(value)) => *value != 0.0,
            Some(OptionValue::Str(value)) => matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "y" | "on"
            ),
            None => default,
        }
    }

    pub fn int(&self, key: &str, default: i64) -> i64 {
        match self.0.get(key) {
            Some(OptionValue::Int(value)) => *value,
            Some(OptionValue::Float(value)) => *value as i64,
            Some(OptionValue::Bool(value)) => i64::from(*value),
            Some(OptionValue::Str(value)) => value.trim().parse().unwrap_or(default),
            None => default,
        }
    }

    pub fn float(&self, key: &str, default: f64) -> f64 {
        match self.0.get(key) {
            Some(OptionValue::Float(value)) => *value,
            Some(OptionValue::Int(value)) => *value as f64,
            Some(OptionValue::Bool(value)) => f64::from(u8::from(*value)),
            Some(OptionValue::Str(value)) => value.trim().parse().unwrap_or(default),
            None => default,
        }
    }

    pub fn str(&self, key: &str, default: &str) -> String {
        match self.0.get(key) {
            Some(OptionValue::Str(value)) => value.clone(),
            Some(other) => other.to_display_string(),
            None => default.to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Option mixins (`libresvip.model.option_mixins`)
// ---------------------------------------------------------------------------

pub fn enable_instrumental_track_importation() -> OptionFieldDef {
    OptionFieldDef::bool(
        "import_instrumental_track",
        "Import instrumental tracks",
        true,
    )
}

pub fn enable_pitch_importation() -> OptionFieldDef {
    OptionFieldDef::bool("import_pitch", "Import pitch curve", true)
}

pub fn enable_volume_importation() -> OptionFieldDef {
    OptionFieldDef::bool("import_volume", "Import dynamics envelope", true)
}

pub fn enable_breath_importation() -> OptionFieldDef {
    OptionFieldDef::bool("import_breath", "Import breath envelope", true)
}

pub fn enable_gender_importation() -> OptionFieldDef {
    OptionFieldDef::bool("import_gender", "Import gender envelope", true)
}

pub fn enable_strength_importation() -> OptionFieldDef {
    OptionFieldDef::bool("import_strength", "Import strength envelope", true)
}

pub fn static_tempo() -> OptionFieldDef {
    OptionFieldDef::float("tempo", "Constant tempo", crate::model::base::DEFAULT_BPM)
        .with_description(
            "Use this tempo to reset time axis of projects with dynamic tempos or without tempos",
        )
}

pub fn select_single_track() -> OptionFieldDef {
    OptionFieldDef::int("track_index", "Track index", -1)
        .with_description("Start from 0, -1 means automatic selection")
}

pub fn extract_embedded_audio() -> OptionFieldDef {
    OptionFieldDef::bool("extract_audio", "Extract audio", false).with_description(
        "Extract audio from project file for later use if the path does not exist \
         (this option has the side effect of writing additional files)",
    )
}
