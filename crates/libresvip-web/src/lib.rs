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

use libresvip_core::options::{OptionFieldDef, Options};

/// Holds the payload of the last call: the converted file, an error message or
/// the format list, depending on which export filled it.
static RESULT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn defaults(defs: &[OptionFieldDef]) -> Options {
    Options::from_pairs(defs.iter().map(|def| (def.key.to_owned(), def.default.clone())))
}

fn run(input: &[u8], input_format: &str, output_format: &str) -> Result<Vec<u8>, String> {
    let input_plugin = libresvip_plugins::plugin_by_value(input_format)
        .ok_or_else(|| format!("unknown input format: {input_format}"))?;
    let output_plugin = libresvip_plugins::plugin_by_value(output_format)
        .ok_or_else(|| format!("unknown output format: {output_format}"))?;

    // The plugins take paths; on wasm they are served by `libresvip_core::io`
    // from memory, so any name works.
    let input_path = Path::new("input.dat").to_path_buf();
    let output_path = Path::new("output.dat").to_path_buf();
    libresvip_core::io::write(&input_path, input).map_err(|error| error.to_string())?;

    let project = input_plugin
        .load(&input_path, &defaults(&input_plugin.input_options()))
        .map_err(|error| error.to_string())?;
    output_plugin
        .dump(&output_path, &project, &defaults(&output_plugin.output_options()))
        .map_err(|error| error.to_string())?;

    libresvip_core::io::read(&output_path).map_err(|error| error.to_string())
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
        plugins.push(format!(
            "{{\"id\":\"{}\",\"name\":\"{}\",\"description\":\"{}\",\"suffixes\":[{}]}}",
            escape(info.id),
            escape(info.name),
            escape(info.description),
            suffixes.join(",")
        ));
    }
    *RESULT.lock().unwrap() = format!("[{}]", plugins.join(",")).into_bytes();
    0
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
}
