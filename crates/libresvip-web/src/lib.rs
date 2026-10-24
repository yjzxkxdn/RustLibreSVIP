//! WebAssembly build of the conversion engine.
//!
//! There is no `wasm-bindgen` and no WASI: the module has **zero imports**, so a
//! plain `WebAssembly.instantiate(bytes, {})` works and any host language can
//! drive it through the small C ABI below.
//!
//! ```js
//! const { instance } = await WebAssembly.instantiate(bytes, {});
//! const api = instance.exports;
//! api.formats_json();                       // -> JSON in the result buffer
//! const pointer = api.alloc(file.length);
//! new Uint8Array(api.memory.buffer, pointer, file.length).set(file);
//! const status = api.convert(pointer, file.length, idPtr, idLen, outPtr, outLen);
//! ```
//!
//! Known limitations of this entry point: one input file, one output file, the
//! default options of both plugins, and no audio-track probing. `split`
//! (one file per chunk), `merge` and custom options still live in the CLI/GUI.

use std::path::Path;
use std::sync::Mutex;

use libresvip_core::core::warning_types::take_typed_warnings;
use libresvip_core::options::{OptionFieldDef, Options};

/// Holds the payload of the last call: the converted file, an error message or
/// the format list, depending on which export filled it.
static RESULT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// JSON array of the structured warnings raised by the last `convert` call,
/// e.g. `[{"kind":"tempo dropped","detail":"…"}]`.
static LAST_WARNINGS: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn defaults(defs: &[OptionFieldDef]) -> Options {
    Options::from_pairs(defs.iter().map(|def| (def.key.to_owned(), def.default.clone())))
}

fn run(input: &[u8], input_format: &str, output_format: &str) -> Result<Vec<u8>, String> {
    let input_plugin = libresvip_plugins::plugin_by_value(input_format)
        .ok_or_else(|| format!("unknown input format: {input_format}"))?;
    let output_plugin = libresvip_plugins::plugin_by_value(output_format)
        .ok_or_else(|| format!("unknown output format: {output_format}"))?;
    run_with(
        input,
        input_format,
        output_format,
        &defaults(&input_plugin.input_options()),
        &defaults(&output_plugin.output_options()),
    )
}

fn run_with(
    input: &[u8],
    input_format: &str,
    output_format: &str,
    input_options: &Options,
    output_options: &Options,
) -> Result<Vec<u8>, String> {
    let input_plugin = libresvip_plugins::plugin_by_value(input_format)
        .ok_or_else(|| format!("unknown input format: {input_format}"))?;
    let output_plugin = libresvip_plugins::plugin_by_value(output_format)
        .ok_or_else(|| format!("unknown output format: {output_format}"))?;

    // The plugins take paths; on wasm they are served by `libresvip_core::io`
    // from memory, so any name works.
    let input_path = Path::new("input.dat").to_path_buf();
    let output_path = Path::new("output.dat").to_path_buf();

    // The virtual files must not outlive the call: wasm linear memory only
    // grows, so dropping them here keeps repeated conversions from doubling
    // the resident project data.
    let outcome = convert_via_files(input, input_plugin, output_plugin, &input_path, &output_path, input_options, output_options);
    let _ = libresvip_core::io::remove_file(&input_path);
    let _ = libresvip_core::io::remove_file(&output_path);
    outcome
}

fn convert_via_files(
    input: &[u8],
    input_plugin: &'static dyn libresvip_core::plugin::SvsPlugin,
    output_plugin: &'static dyn libresvip_core::plugin::SvsPlugin,
    input_path: &Path,
    output_path: &Path,
    input_options: &Options,
    output_options: &Options,
) -> Result<Vec<u8>, String> {
    libresvip_core::io::write(input_path, input).map_err(|error| error.to_string())?;

    let project = input_plugin
        .load(input_path, input_options)
        .map_err(|error| error.to_string())?;
    output_plugin
        .dump(output_path, &project, output_options)
        .map_err(|error| error.to_string())?;

    libresvip_core::io::read(output_path).map_err(|error| error.to_string())
}

/// Serializes the typed warnings raised so far into `LAST_WARNINGS`.
fn record_warnings_json() {
    let entries: Vec<String> = take_typed_warnings()
        .into_iter()
        .map(|warning| {
            format!(
                "{{\"kind\":\"{}\",\"detail\":\"{}\"}}",
                escape(warning.kind.label()),
                escape(&warning.detail)
            )
        })
        .collect();
    *LAST_WARNINGS.lock().unwrap() = format!("[{}]", entries.join(",")).into_bytes();
}

/// Reserves `size` bytes and hands the pointer to the host.
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(size);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// Releases a buffer obtained from [`alloc`].
///
/// # Safety
/// `pointer` must come from [`alloc`] with the same `size`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(pointer: *mut u8, size: usize) {
    drop(Vec::from_raw_parts(pointer, 0, size));
}

/// # Safety
/// The pointer must reference `len` readable UTF-8 bytes.
unsafe fn text(pointer: *const u8, len: usize) -> Result<String, String> {
    std::str::from_utf8(std::slice::from_raw_parts(pointer, len))
        .map(str::to_owned)
        .map_err(|error| format!("format name is not UTF-8: {error}"))
}

/// Converts one project file.
///
/// Returns 0 on success and 1 on failure; either way the payload (converted
/// file or error message) is available through [`result_ptr`] / [`result_len`].
///
/// # Safety
/// Every pointer must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn convert(
    input_pointer: *const u8,
    input_len: usize,
    input_format_pointer: *const u8,
    input_format_len: usize,
    output_format_pointer: *const u8,
    output_format_len: usize,
) -> i32 {
    let input = std::slice::from_raw_parts(input_pointer, input_len);
    let attempt = text(input_format_pointer, input_format_len).and_then(|input_format| {
        text(output_format_pointer, output_format_len)
            .and_then(|output_format| run(input, &input_format, &output_format))
    });
    // Whatever happened, the warnings of this conversion (if any) are
    // available to the host through `warnings_json`.
    record_warnings_json();
    match attempt {
        Ok(bytes) => {
            *RESULT.lock().unwrap() = bytes;
            0
        }
        Err(message) => {
            *RESULT.lock().unwrap() = message.into_bytes();
            1
        }
    }
}

/// Fills the result buffer with `[{"id","name","description","suffixes"}]` for
/// every registered format and returns 0.
#[no_mangle]
pub extern "C" fn formats_json() -> i32 {
    let mut plugins: Vec<String> = Vec::new();
    for plugin in libresvip_plugins::all_plugins() {
        let info = plugin.info();
        let suffixes: Vec<String> = info
            .suffixes
            .iter()
            .map(|suffix| format!("\"{}\"", escape(suffix)))
            .collect();
        let caps = libresvip_plugins::capabilities::capabilities_by_id(info.id)
            .unwrap_or_default();
        let features: Vec<String> = caps
            .features
            .iter()
            .map(|feature| format!("\"{}\"", escape(feature.key())))
            .collect();
        let max_tracks = caps
            .max_tracks_per_file
            .map(|count| count.to_string())
            .unwrap_or_else(|| "null".to_owned());
        plugins.push(format!(
            "{{\"id\":\"{}\",\"name\":\"{}\",\"description\":\"{}\",\"suffixes\":[{}],\"features\":[{}],\"max_tracks_per_file\":{}}}",
            escape(info.id),
            escape(info.name),
            escape(info.description),
            suffixes.join(","),
            features.join(","),
            max_tracks
        ));
    }
    *RESULT.lock().unwrap() = format!("[{}]", plugins.join(",")).into_bytes();
    0
}

/// Fills the result buffer with the JSON array of structured warnings raised
/// by the last [`convert`] call (`[{"kind","detail"}]`) and returns 0.
#[no_mangle]
pub extern "C" fn warnings_json() -> i32 {
    *RESULT.lock().unwrap() = LAST_WARNINGS.lock().unwrap().clone();
    0
}

/// Serializes the option schemas of one format pair as
/// `{"input":[...],"output":[...]}` into the result buffer; each field is
/// `{"key","title","kind","typeName","default","description","choices"}`.
///
/// # Safety
/// The pointers must reference `len` readable UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn options_json(
    input_format_pointer: *const u8,
    input_format_len: usize,
    output_format_pointer: *const u8,
    output_format_len: usize,
) -> i32 {
    let attempt = text(input_format_pointer, input_format_len).and_then(|input_format| {
        text(output_format_pointer, output_format_len)
            .and_then(|output_format| {
                let input_plugin = libresvip_plugins::plugin_by_value(&input_format)
                    .ok_or_else(|| format!("unknown input format: {input_format}"))?;
                let output_plugin = libresvip_plugins::plugin_by_value(&output_format)
                    .ok_or_else(|| format!("unknown output format: {output_format}"))?;
                Ok(format!(
                    "{{\"input\":[{}],\"output\":[{}]}}",
                    schema_fields(&input_plugin.input_options()).join(","),
                    schema_fields(&output_plugin.output_options()).join(","),
                ))
            })
    });
    match attempt {
        Ok(json) => {
            *RESULT.lock().unwrap() = json.into_bytes();
            0
        }
        Err(message) => {
            *RESULT.lock().unwrap() = message.into_bytes();
            1
        }
    }
}

fn schema_fields(fields: &[OptionFieldDef]) -> Vec<String> {
    fields
        .iter()
        .map(|field| {
            let kind = match &field.kind {
                libresvip_core::options::OptionKind::Bool => "bool",
                libresvip_core::options::OptionKind::Int => "int",
                libresvip_core::options::OptionKind::Float => "float",
                libresvip_core::options::OptionKind::Str => "str",
                libresvip_core::options::OptionKind::Enum { .. } => "enum",
                libresvip_core::options::OptionKind::Complex { .. } => "complex",
                libresvip_core::options::OptionKind::Opaque { .. } => "opaque",
            };
            let choices = match &field.kind {
                libresvip_core::options::OptionKind::Enum { variants, .. } => variants
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

/// Parses a JSON object of option overrides into an [`Options`] typed by the
/// plugin's schema (fields absent from the JSON keep their schema default).
fn options_from_json(schema: &[OptionFieldDef], json: &str) -> Result<Options, String> {
    let value: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|error| format!("invalid options json: {error}"))?;
    let mut options = defaults(schema);
    for field in schema {
        let Some(raw) = value.get(field.key) else {
            continue;
        };
        let parsed = match (&field.kind, raw) {
            (libresvip_core::options::OptionKind::Bool, serde_json::Value::Bool(flag)) => {
                Some(libresvip_core::options::OptionValue::Bool(*flag))
            }
            (
                libresvip_core::options::OptionKind::Bool,
                serde_json::Value::Number(number),
            ) => number
                .as_i64()
                .map(|value| libresvip_core::options::OptionValue::Bool(value != 0)),
            (libresvip_core::options::OptionKind::Int, serde_json::Value::Number(number)) => {
                number.as_i64().map(libresvip_core::options::OptionValue::Int)
            }
            (libresvip_core::options::OptionKind::Float, serde_json::Value::Number(number)) => number
                .as_f64()
                .map(libresvip_core::options::OptionValue::Float),
            (
                libresvip_core::options::OptionKind::Enum { .. },
                serde_json::Value::String(choice),
            ) => Some(libresvip_core::options::OptionValue::Str(choice.clone())),
            (_, serde_json::Value::String(text)) => {
                Some(libresvip_core::options::OptionValue::Str(text.clone()))
            }
            _ => None,
        };
        if let Some(parsed) = parsed {
            options.insert(field.key, parsed);
        }
    }
    Ok(options)
}

/// [`convert`] with explicit option objects (JSON) for both plugins.
///
/// # Safety
/// Every pointer must reference `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn convert_with_options(
    input_pointer: *const u8,
    input_len: usize,
    input_format_pointer: *const u8,
    input_format_len: usize,
    output_format_pointer: *const u8,
    output_format_len: usize,
    input_options_pointer: *const u8,
    input_options_len: usize,
    output_options_pointer: *const u8,
    output_options_len: usize,
) -> i32 {
    let input = std::slice::from_raw_parts(input_pointer, input_len);
    let attempt = text(input_format_pointer, input_format_len).and_then(|input_format| {
        text(output_format_pointer, output_format_len).and_then(|output_format| {
            text(input_options_pointer, input_options_len).and_then(|input_json| {
                text(output_options_pointer, output_options_len).and_then(|output_json| {
                    let input_options =
                        options_from_json(&plugin_schema(&input_format, true), &input_json)?;
                    let output_options =
                        options_from_json(&plugin_schema(&output_format, false), &output_json)?;
                    run_with(input, &input_format, &output_format, &input_options, &output_options)
                })
            })
        })
    });
    record_warnings_json();
    match attempt {
        Ok(bytes) => {
            *RESULT.lock().unwrap() = bytes;
            0
        }
        Err(message) => {
            *RESULT.lock().unwrap() = message.into_bytes();
            1
        }
    }
}

fn plugin_schema(format: &str, input: bool) -> Vec<OptionFieldDef> {
    let Some(plugin) = libresvip_plugins::plugin_by_value(format) else {
        return Vec::new();
    };
    if input {
        plugin.input_options()
    } else {
        plugin.output_options()
    }
}

#[no_mangle]
pub extern "C" fn result_ptr() -> *const u8 {
    RESULT.lock().unwrap().as_ptr()
}

#[no_mangle]
pub extern "C" fn result_len() -> usize {
    RESULT.lock().unwrap().len()
}

/// Minimal JSON string escaping for the format metadata.
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

    #[test]
    fn every_registered_format_is_listed() {
        assert_eq!(formats_json(), 0);
        let json = String::from_utf8(RESULT.lock().unwrap().clone()).expect("utf-8");
        assert!(json.starts_with('[') && json.ends_with(']'));
        for plugin in libresvip_plugins::all_plugins() {
            assert!(
                json.contains(&format!("\"id\":\"{}\"", plugin.info().id)),
                "{} is missing from formats_json",
                plugin.info().id
            );
        }
    }

    #[test]
    fn converts_a_project_through_the_memory_file_system() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("testdata")
            .join("simple.json");
        let input = std::fs::read(&source).expect("read testdata/simple.json");
        let output = run(&input, "json", "ust").expect("json -> ust works");
        // On native targets the same code writes real files next to the crate.
        let _ = std::fs::remove_file("input.dat");
        let _ = std::fs::remove_file("output.dat");
        assert!(!output.is_empty(), "the output file must not be empty");
        assert!(
            String::from_utf8_lossy(&output).contains("Tempo"),
            "the UST output should look like a UST file"
        );
    }

    #[test]
    fn unknown_formats_are_reported() {
        let error = run(b"", "nope", "acep").expect_err("unknown input format");
        assert!(error.contains("unknown input format"));
    }

    #[test]
    fn formats_json_carries_capabilities() {
        assert_eq!(formats_json(), 0);
        let json = String::from_utf8(RESULT.lock().unwrap().clone()).expect("utf-8");
        assert!(
            json.contains("\"features\":[\"pitch\",\"volume\",\"breath\",\"gender\",\"strength\""),
            "the full-curve formats must declare all five features: {json}"
        );
        assert!(
            json.contains("\"features\":[\"pitch\"],\"max_tracks_per_file\":1}"),
            "ust/svg are declared as pitch-only single-track: {json}"
        );
    }

    #[test]
    fn warnings_json_reports_the_last_conversion() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("testdata")
            .join("simple.json");
        let input = std::fs::read(&source).expect("read testdata/simple.json");
        run(&input, "json", "ust").expect("json -> ust works");
        record_warnings_json();
        assert_eq!(warnings_json(), 0);
        let json = String::from_utf8(RESULT.lock().unwrap().clone()).expect("utf-8");
        assert!(json.starts_with('[') && json.ends_with(']'), "array: {json}");
    }
}
