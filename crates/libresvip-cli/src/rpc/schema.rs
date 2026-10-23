//! `json_schema` generation for `PluginInfos`.
//!
//! Python builds the schema with pydantic (`model_json_schema`) and a custom
//! `GettextGenerateJsonSchema` generator that translates every `title` and
//! `description`. Rust has no reflection, but each plugin already exposes its
//! pydantic fields as [`OptionFieldDef`] lists for the CLI prompts, which is
//! exactly the information pydantic needs. The shapes produced here mirror
//! pydantic's compact output:
//!
//! * keys are sorted, except inside `properties` (declaration order is kept);
//! * enum types become `$defs` entries referenced through `$ref`;
//! * `_value_`-annotated enums additionally get a translated `oneOf` block;
//! * `json.dumps` runs with `ensure_ascii=True` and compact separators, matching
//!   `libresvip.core.compat.json`.

use libresvip_core::options::{OptionFieldDef, OptionKind, OptionValue};
use libresvip_core::utils::translation::TranslationChain;
use serde_json::{Map, Number, Value};

/// One option class → pydantic-compatible JSON schema string.
pub fn option_schema(
    plugin_id: &str,
    fields: &[OptionFieldDef],
    translation: &TranslationChain,
) -> String {
    let mut definitions: std::collections::BTreeMap<String, Value> =
        std::collections::BTreeMap::new();
    let mut properties = Map::new();

    for field in fields {
        if let OptionKind::Enum {
            type_name,
            variants,
        } = &field.kind
        {
            definitions
                .entry((*type_name).to_owned())
                .or_insert_with(|| {
                    enum_definition(type_name, variants, field.show_enum_values, translation)
                });
        }
        properties.insert(
            field.key.to_owned(),
            property_schema(plugin_id, field, translation),
        );
    }

    let mut root = Map::new();
    if !definitions.is_empty() {
        let mut defs = Map::new();
        for (name, value) in definitions {
            defs.insert(name, value);
        }
        root.insert("$defs".to_owned(), Value::Object(defs));
    }
    root.insert("properties".to_owned(), Value::Object(properties));
    root.insert("type".to_owned(), Value::String("object".to_owned()));
    to_json_string(&Value::Object(root))
}

/// Builds an object whose keys are already in the order Python emits.
fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key.to_owned(), value);
    }
    Value::Object(map)
}

/// Whether an enum's members are integers (pydantic `type: "integer"`).
pub(crate) fn integer_enum(variants: &[libresvip_core::options::EnumVariant]) -> bool {
    !variants.is_empty()
        && variants
            .iter()
            .all(|variant| variant.value.parse::<i64>().is_ok())
}

fn enum_member_value(value: &str, is_integer: bool) -> Value {
    if is_integer {
        match value.parse::<i64>() {
            Ok(number) => Value::Number(Number::from(number)),
            Err(_) => Value::String(value.to_owned()),
        }
    } else {
        Value::String(value.to_owned())
    }
}

fn enum_definition(
    type_name: &str,
    variants: &[libresvip_core::options::EnumVariant],
    with_titles: bool,
    translation: &TranslationChain,
) -> Value {
    let is_integer = integer_enum(variants);
    let enum_values = variants
        .iter()
        .map(|variant| enum_member_value(variant.value, is_integer))
        .collect();
    let mut entries = vec![("enum", Value::Array(enum_values))];
    if with_titles {
        let one_of = variants
            .iter()
            .map(|variant| {
                object(vec![
                    ("const", enum_member_value(variant.value, is_integer)),
                    ("title", Value::String(translation.gettext(variant.title))),
                ])
            })
            .collect();
        entries.push(("oneOf", Value::Array(one_of)));
    }
    entries.push(("title", Value::String(translation.gettext(type_name))));
    entries.push((
        "type",
        Value::String(if is_integer { "integer" } else { "string" }.to_owned()),
    ));
    object(entries)
}

fn default_value(value: &OptionValue, is_integer: bool) -> Value {
    match value {
        OptionValue::Bool(value) => Value::Bool(*value),
        OptionValue::Int(value) => Value::Number(Number::from(*value)),
        OptionValue::Float(value) => Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        OptionValue::Str(value) => {
            if is_integer {
                match value.parse::<i64>() {
                    Ok(number) => Value::Number(Number::from(number)),
                    Err(_) => Value::String(value.clone()),
                }
            } else {
                Value::String(value.clone())
            }
        }
    }
}

fn property_schema(
    plugin_id: &str,
    field: &OptionFieldDef,
    translation: &TranslationChain,
) -> Value {
    // pydantic sorts the keys of each property schema.
    let mut entries: Vec<(&str, Value)> = vec![("default", default_value(&field.default, false))];
    if let Some(description) = field.description {
        entries.push((
            "description",
            Value::String(translation.gettext(description)),
        ));
    }
    match &field.kind {
        OptionKind::Enum {
            type_name,
            variants,
        } => {
            let is_integer = integer_enum(variants);
            if let Some(default) = entries.get_mut(0) {
                default.1 = default_value(&field.default, is_integer);
            }
            entries.insert(0, ("$ref", Value::String(format!("#/$defs/{type_name}"))));
        }
        OptionKind::Bool => entries.push(("type", Value::String("boolean".to_owned()))),
        OptionKind::Int => entries.push(("type", Value::String("integer".to_owned()))),
        OptionKind::Float => entries.push(("type", Value::String("number".to_owned()))),
        OptionKind::Str => entries.push(("type", Value::String("string".to_owned()))),
        OptionKind::Opaque { type_name } => {
            if *type_name == "Color" {
                entries.push(("format", Value::String("color".to_owned())));
            }
            entries.push(("type", Value::String("string".to_owned())));
        }
        // `BaseComplexModel` subclasses (`NormalizationArgument`) expose a
        // custom `json_schema_extra` finalizer in Python that renders them as a
        // plain string.
        OptionKind::Complex { .. } => entries.push(("type", Value::String("string".to_owned()))),
    }
    if let Some(choices) = field.choices {
        let values = choices
            .iter()
            .map(|choice| Value::String((*choice).to_owned()))
            .collect();
        entries.push(("enum", Value::Array(values)));
    } else if field.key == "encoding" && plugin_id == "ass" {
        // The ASS options port omits `json_schema_extra={"enum": ...}` for
        // `encoding`; Python (and the CLI prompt) still offer every supported
        // charset. Restore it here because the plugin metadata is out of scope.
        let values = libresvip_core::utils::text::supported_charset_names()
            .iter()
            .map(|choice| Value::String((*choice).to_owned()))
            .collect();
        entries.push(("enum", Value::Array(values)));
    }
    entries.push(("title", Value::String(translation.gettext(field.title))));
    // Keep the keys sorted the way pydantic's `_sort_json_schema` does.
    entries.sort_by(|left, right| left.0.cmp(right.0));
    object(entries)
}

// ---------------------------------------------------------------------------
// `json.dumps(..., ensure_ascii=True)`-compatible serialiser
// ---------------------------------------------------------------------------

/// Serialises with the exact escaping/formatting of Python's stdlib `json`
/// configured by `libresvip.core.compat` (compact separators, `ensure_ascii`
/// left at its default `True`).
pub fn to_json_string(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out);
    out
}

fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::String(text) => write_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write_value(item, out);
            }
            out.push('}');
        }
    }
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{9}' => out.push_str("\\t"),
            '\u{a}' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\u{d}' => out.push_str("\\r"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character if (character as u32) < 0x7f => out.push(character),
            character => {
                let mut code_point = character as u32;
                if code_point <= 0xFFFF {
                    out.push_str(&format!("\\u{code_point:04x}"));
                } else {
                    code_point -= 0x10000;
                    let high = 0xD800 + (code_point >> 10);
                    let low = 0xDC00 + (code_point & 0x3FF);
                    out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
                }
            }
        }
    }
    out.push('"');
}
