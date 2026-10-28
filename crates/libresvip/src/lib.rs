//! Safe, byte-oriented API for the LibreSVIP conversion engine.
//!
//! This is the crate to depend on when embedding the converter in another Rust
//! program, and it is the single implementation behind the C ABI in
//! `libresvip-ffi` and the WebAssembly entry point in `libresvip-web`.
//!
//! ```no_run
//! let input = std::fs::read("song.ustx")?;
//! let output = libresvip::convert_bytes(&input, "ustx", "svp", None, None)?;
//! std::fs::write("song.svp", output)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Every entry point takes and returns bytes: the plugins themselves work with
//! paths, so a conversion stages the input in a uniquely named temporary file,
//! lets the plugin run, and reads the result back. On `wasm32` those paths live
//! in the in-memory table of `libresvip_core::io`, so the same code works in a
//! browser without a file system.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use libresvip_core::core::warning_types::take_typed_warnings;
use libresvip_core::options::{OptionFieldDef, OptionKind, OptionValue, Options};

pub use libresvip_core::options::{OptionKind as FieldKind, Options as ConversionOptions};

/// Version of the engine, taken from the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Every failure this crate reports is a message: the plugins already produce
/// human-readable text and the FFI boundary can only carry that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// One structured warning raised while converting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub kind: String,
    pub detail: String,
}

/// Result of a conversion: the output file plus everything the plugins warned
/// about. Callers that only want the file can ignore `warnings`.
#[derive(Debug, Clone)]
pub struct Converted {
    pub bytes: Vec<u8>,
    pub warnings: Vec<Warning>,
}

/// What a registered format can do, as reported by [`formats_json`].
#[derive(Debug, Clone)]
pub struct FormatInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub suffixes: Vec<String>,
    pub features: Vec<String>,
    pub max_tracks_per_file: Option<usize>,
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn scratch_path(suffix: &str) -> PathBuf {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut path = std::env::temp_dir();
        path.push(format!("libresvip-{}-{sequence}.{suffix}", std::process::id()));
        path
    }
    #[cfg(target_arch = "wasm32")]
    {
        // The wasm build has no temp directory: `libresvip_core::io` serves
        // these names from memory, so a relative name is enough.
        PathBuf::from(format!("libresvip-{sequence}.{suffix}"))
    }
}

/// Removes a staged file even when the conversion returns early.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = libresvip_core::io::remove_file(&self.0);
    }
}

/// Converts `input` from one format to another with the plugins' default
/// options.
pub fn convert_bytes(
    input: &[u8],
    input_format: &str,
    output_format: &str,
    input_options: Option<&Options>,
    output_options: Option<&Options>,
) -> Result<Vec<u8>, Error> {
    convert_with_warnings(input, input_format, output_format, input_options, output_options)
        .map(|converted| converted.bytes)
}

/// [`convert_bytes`], keeping the warnings raised during the conversion.
pub fn convert_with_warnings(
    input: &[u8],
    input_format: &str,
    output_format: &str,
    input_options: Option<&Options>,
    output_options: Option<&Options>,
) -> Result<Converted, Error> {
    let input_plugin = libresvip_plugins::plugin_by_value(input_format)
        .ok_or_else(|| Error(format!("unknown input format: {input_format}")))?;
    let output_plugin = libresvip_plugins::plugin_by_value(output_format)
        .ok_or_else(|| Error(format!("unknown output format: {output_format}")))?;

    let input_options = input_options
        .cloned()
        .unwrap_or_else(|| default_options(&input_plugin.input_options()));
    let output_options = output_options
        .cloned()
        .unwrap_or_else(|| default_options(&output_plugin.output_options()));

    let input_path = scratch_path("in");
    let output_path = scratch_path("out");
    let _input_guard = Scratch(input_path.clone());
    let _output_guard = Scratch(output_path.clone());

    libresvip_core::io::write(&input_path, input).map_err(|error| Error(error.to_string()))?;
    let project = input_plugin
        .load(&input_path, &input_options)
        .map_err(|error| Error(error.to_string()))?;
    output_plugin
        .dump(&output_path, &project, &output_options)
        .map_err(|error| Error(error.to_string()))?;
    let bytes = libresvip_core::io::read(&output_path).map_err(|error| Error(error.to_string()))?;

    let warnings = take_typed_warnings()
        .into_iter()
        .map(|warning| Warning {
            kind: warning.kind.label().to_owned(),
            detail: warning.detail,
        })
        .collect();

    Ok(Converted { bytes, warnings })
}

/// The plugin's own defaults as a concrete [`Options`] map.
pub fn default_options(fields: &[OptionFieldDef]) -> Options {
    Options::from_pairs(fields.iter().map(|field| (field.key.to_owned(), field.default.clone())))
}

/// Turns a JSON object of overrides (`{"key": value}`) into [`Options`], typed
/// by the plugin schema. Unknown keys are ignored; absent keys keep defaults.
pub fn options_from_json(schema: &[OptionFieldDef], json: &str) -> Result<Options, Error> {
    let value: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|error| Error(format!("invalid options json: {error}")))?;
    let mut options = default_options(schema);
    for field in schema {
        let Some(raw) = value.get(field.key) else {
            continue;
        };
        let parsed = match (&field.kind, raw) {
            (OptionKind::Bool, serde_json::Value::Bool(flag)) => Some(OptionValue::Bool(*flag)),
            (OptionKind::Bool, serde_json::Value::Number(number)) => {
                number.as_i64().map(|value| OptionValue::Bool(value != 0))
            }
            (OptionKind::Int, serde_json::Value::Number(number)) => {
                number.as_i64().map(OptionValue::Int)
            }
            (OptionKind::Float, serde_json::Value::Number(number)) => {
                number.as_f64().map(OptionValue::Float)
            }
            (OptionKind::Enum { .. }, serde_json::Value::String(choice)) => {
                Some(OptionValue::Str(choice.clone()))
            }
            (_, serde_json::Value::String(text)) => Some(OptionValue::Str(text.clone())),
            _ => None,
        };
        if let Some(parsed) = parsed {
            options.insert(field.key, parsed);
        }
    }
    Ok(options)
}

/// Option schema of one format, as declared by its plugin.
pub fn option_fields(format: &str) -> (Vec<OptionFieldDef>, Vec<OptionFieldDef>) {
    let Some(plugin) = libresvip_plugins::plugin_by_value(format) else {
        return (Vec::new(), Vec::new());
    };
    (plugin.input_options(), plugin.output_options())
}

// ---------------------------------------------------------------------------
// Metadata as JSON (the same payloads the wasm module exposes)
// ---------------------------------------------------------------------------

/// Every registered format, in registration order.
pub fn formats() -> Vec<FormatInfo> {
    libresvip_plugins::all_plugins()
        .iter()
        .map(|plugin| {
            let info = plugin.info();
            let capabilities =
                libresvip_plugins::capabilities::capabilities_by_id(info.id).unwrap_or_default();
            FormatInfo {
                id: info.id.to_owned(),
                name: info.name.to_owned(),
                description: info.description.to_owned(),
                suffixes: info.suffixes.iter().map(|suffix| (*suffix).to_owned()).collect(),
                features: capabilities
                    .features
                    .iter()
                    .map(|feature| feature.key().to_owned())
                    .collect(),
                max_tracks_per_file: capabilities.max_tracks_per_file,
            }
        })
        .collect()
}

/// `[{"id","name","description","suffixes","features","max_tracks_per_file"}]`.
pub fn formats_json() -> String {
    let entries: Vec<String> = formats()
        .into_iter()
        .map(|format| {
            let suffixes: Vec<String> = format
                .suffixes
                .iter()
                .map(|suffix| format!("\"{}\"", escape(suffix)))
                .collect();
            let features: Vec<String> = format
                .features
                .iter()
                .map(|feature| format!("\"{}\"", escape(feature)))
                .collect();
            let max_tracks = format
                .max_tracks_per_file
                .map(|count| count.to_string())
                .unwrap_or_else(|| "null".to_owned());
            format!(
                "{{\"id\":\"{}\",\"name\":\"{}\",\"description\":\"{}\",\"suffixes\":[{}],\"features\":[{}],\"max_tracks_per_file\":{}}}",
                escape(&format.id),
                escape(&format.name),
                escape(&format.description),
                suffixes.join(","),
                features.join(","),
                max_tracks
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// `{"input":[...],"output":[...]}` describing the option schema of a pair.
pub fn options_json(input_format: &str, output_format: &str) -> Result<String, Error> {
    let input_plugin = libresvip_plugins::plugin_by_value(input_format)
        .ok_or_else(|| Error(format!("unknown input format: {input_format}")))?;
    let output_plugin = libresvip_plugins::plugin_by_value(output_format)
        .ok_or_else(|| Error(format!("unknown output format: {output_format}")))?;
    Ok(format!(
        "{{\"input\":[{}],\"output\":[{}]}}",
        schema_fields(&input_plugin.input_options()).join(","),
        schema_fields(&output_plugin.output_options()).join(","),
    ))
}

fn schema_fields(fields: &[OptionFieldDef]) -> Vec<String> {
    fields
        .iter()
        .map(|field| {
            let kind = match &field.kind {
                OptionKind::Bool => "bool",
                OptionKind::Int => "int",
                OptionKind::Float => "float",
                OptionKind::Str => "str",
                OptionKind::Enum { .. } => "enum",
                OptionKind::Complex { .. } => "complex",
                OptionKind::Opaque { .. } => "opaque",
            };
            let choices = match &field.kind {
                OptionKind::Enum { variants, .. } => variants
                    .iter()
                    .map(|variant| {
                        format!(
                            "{{\"value\":\"{}\",\"title\":\"{}\"}}",
                            escape(&variant.value),
                            escape(variant.title)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(","),
                _ => String::new(),
            };
            format!(
                "{{\"key\":\"{}\",\"title\":\"{}\",\"kind\":\"{kind}\",\"default\":\"{}\",\"description\":\"{}\",\"choices\":[{choices}]}}",
                escape(field.key),
                escape(field.title),
                escape(&field.default.to_display_string()),
                field.description.map(escape).unwrap_or_default(),
            )
        })
        .collect()
}

/// `[{"kind","detail"}]` for a list of warnings.
pub fn warnings_json(warnings: &[Warning]) -> String {
    let entries: Vec<String> = warnings
        .iter()
        .map(|warning| {
            format!(
                "{{\"kind\":\"{}\",\"detail\":\"{}\"}}",
                escape(&warning.kind),
                escape(&warning.detail)
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Minimal JSON string escaping for the metadata payloads.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32))
            }
            character => out.push(character),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_project() -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("testdata")
            .join("simple.json");
        std::fs::read(path).expect("testdata/simple.json")
    }

    #[test]
    fn converts_json_to_ust_in_memory() {
        let output = convert_bytes(&simple_project(), "json", "ust", None, None).expect("json -> ust");
        assert!(!output.is_empty());
        assert!(
            String::from_utf8_lossy(&output).contains("Tempo"),
            "the UST output should look like a UST file"
        );
    }

    #[test]
    fn unknown_formats_are_reported() {
        let error = convert_bytes(b"", "nope", "ust", None, None).expect_err("unknown format");
        assert!(error.0.contains("unknown input format"));
    }

    #[test]
    fn formats_json_lists_every_plugin() {
        let json = formats_json();
        assert!(json.starts_with('[') && json.ends_with(']'));
        for plugin in libresvip_plugins::all_plugins() {
            assert!(json.contains(&format!("\"id\":\"{}\"", plugin.info().id)));
        }
    }

    #[test]
    fn options_json_follows_the_plugin_schema() {
        let json = options_json("ustx", "acep").expect("known formats");
        assert!(json.contains("\"input\":[") && json.contains("\"output\":["));
        assert!(json.contains("\"key\":"));
    }

    #[test]
    fn option_overrides_are_typed_and_defaults_kept() {
        let (input_fields, _) = option_fields("ustx");
        let options = options_from_json(&input_fields, "{}").expect("empty overrides");
        assert_eq!(options.len(), input_fields.len());
        assert!(options_from_json(&input_fields, "not json").is_err());
    }

    #[test]
    fn warnings_survive_the_conversion() {
        let converted =
            convert_with_warnings(&simple_project(), "json", "ust", None, None).expect("convert");
        let json = warnings_json(&converted.warnings);
        assert!(json.starts_with('[') && json.ends_with(']'));
    }

    #[test]
    fn scratch_files_are_not_left_behind() {
        let before = std::fs::read_dir(std::env::temp_dir())
            .map(|entries| entries.count())
            .unwrap_or(0);
        let _ = convert_bytes(&simple_project(), "json", "ust", None, None).expect("convert");
        let after = std::fs::read_dir(std::env::temp_dir())
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(before, after, "a staged file was left in the temp directory");
    }
}
