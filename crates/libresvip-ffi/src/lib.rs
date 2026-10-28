//! C ABI for the LibreSVIP conversion engine.
//!
//! The same symbols are exported by the WebAssembly module, so a host that can
//! call a C function (C, C++, C#, Python via `ctypes`, Node via `ffi-napi`,
//! Delphi, …) can convert projects without knowing anything about Rust.
//!
//! Rules for callers:
//!
//! * every returned buffer is owned by the caller and must be released with
//!   [`libresvip_free`] using the length the callee wrote;
//! * strings are NUL-terminated UTF-8, buffers are raw bytes;
//! * return code `0` means success, anything else is an error whose message is
//!   delivered in the same output buffer;
//! * the library is thread-safe: conversions do not share state, and the
//!   warnings belong to the calling thread.

use std::cell::RefCell;
use std::ffi::{c_char, CStr};

const OK: i32 = 0;
const FAILED: i32 = 1;
const BAD_ARGUMENT: i32 = 2;

thread_local! {
    /// Warnings of the last conversion on this thread. `libresvip_convert`
    /// overwrites it, `libresvip_warnings_json` hands it out.
    static LAST_WARNINGS: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Engine version as a static NUL-terminated string.
#[no_mangle]
pub extern "C" fn libresvip_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Releases a buffer previously handed out by this library.
///
/// # Safety
/// `pointer`/`len` must be exactly what the callee returned, and must not be
/// released twice.
#[no_mangle]
pub unsafe extern "C" fn libresvip_free(pointer: *mut u8, len: usize) {
    if pointer.is_null() {
        return;
    }
    let slice = std::ptr::slice_from_raw_parts_mut(pointer, len);
    drop(Box::from_raw(slice));
}

/// Moves `bytes` into the caller's buffer pair.
///
/// # Safety
/// Both pointers must be valid and writable.
unsafe fn hand_over(bytes: Vec<u8>, out_ptr: *mut *mut u8, out_len: *mut usize) {
    if out_ptr.is_null() || out_len.is_null() {
        return;
    }
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let raw = Box::into_raw(boxed);
    *out_ptr = raw as *mut u8;
    *out_len = len;
}

/// # Safety
/// `pointer` must be null or a NUL-terminated C string.
unsafe fn read_string(pointer: *const c_char) -> Result<String, String> {
    if pointer.is_null() {
        return Err("null string argument".to_owned());
    }
    CStr::from_ptr(pointer)
        .to_str()
        .map(str::to_owned)
        .map_err(|error| format!("argument is not UTF-8: {error}"))
}

/// # Safety
/// `pointer` must be null or a NUL-terminated C string.
unsafe fn read_optional_string(pointer: *const c_char) -> Result<Option<String>, String> {
    if pointer.is_null() {
        Ok(None)
    } else {
        read_string(pointer).map(Some)
    }
}

/// `[{"id","name","description","suffixes","features","max_tracks_per_file"}]`.
///
/// # Safety
/// The output pointers must be valid and writable.
#[no_mangle]
pub unsafe extern "C" fn libresvip_formats_json(out_ptr: *mut *mut u8, out_len: *mut usize) -> i32 {
    hand_over(libresvip::formats_json().into_bytes(), out_ptr, out_len);
    OK
}

/// `{"input":[...],"output":[...]}` for one format pair.
///
/// # Safety
/// The string arguments must be valid C strings; the output pointers writable.
#[no_mangle]
pub unsafe extern "C" fn libresvip_options_json(
    input_format: *const c_char,
    output_format: *const c_char,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    let attempt = read_string(input_format).and_then(|input| {
        read_string(output_format).and_then(|output| libresvip::options_json(&input, &output).map_err(|error| error.0))
    });
    match attempt {
        Ok(json) => {
            hand_over(json.into_bytes(), out_ptr, out_len);
            OK
        }
        Err(message) => {
            hand_over(message.into_bytes(), out_ptr, out_len);
            FAILED
        }
    }
}

/// Converts one project file.
///
/// `input_options` and `output_options` are optional JSON objects mapping option
/// keys to values (`{"export_singer": true}`); pass null to use the plugins'
/// defaults.
///
/// # Safety
/// `input` must reference `input_len` readable bytes; the string arguments must
/// be valid C strings; the output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn libresvip_convert(
    input: *const u8,
    input_len: usize,
    input_format: *const c_char,
    output_format: *const c_char,
    input_options: *const c_char,
    output_options: *const c_char,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if input.is_null() && input_len > 0 {
        hand_over("null input buffer".as_bytes().to_vec(), out_ptr, out_len);
        return BAD_ARGUMENT;
    }
    if out_ptr.is_null() || out_len.is_null() {
        return BAD_ARGUMENT;
    }
    let data = std::slice::from_raw_parts(input, input_len);

    let attempt = read_string(input_format).and_then(|input_name| {
        read_string(output_format).and_then(|output_name| {
            read_optional_string(input_options).and_then(|input_json| {
                read_optional_string(output_options).and_then(|output_json| {
                    let (input_fields, _) = libresvip::option_fields(&input_name);
                    let (_, output_fields) = libresvip::option_fields(&output_name);
                    let input_options = match input_json {
                        Some(json) => match libresvip::options_from_json(&input_fields, &json) {
                            Ok(options) => Some(options),
                            Err(error) => return Err(error.0),
                        },
                        None => None,
                    };
                    let output_options = match output_json {
                        Some(json) => match libresvip::options_from_json(&output_fields, &json) {
                            Ok(options) => Some(options),
                            Err(error) => return Err(error.0),
                        },
                        None => None,
                    };
                    libresvip::convert_with_warnings(
                        data,
                        &input_name,
                        &output_name,
                        input_options.as_ref(),
                        output_options.as_ref(),
                    )
                    .map_err(|error| error.0)
                    .map(|converted| {
                        LAST_WARNINGS.with(|slot| {
                            *slot.borrow_mut() = libresvip::warnings_json(&converted.warnings);
                        });
                        converted.bytes
                    })
                })
            })
        })
    });

    match attempt {
        Ok(bytes) => {
            hand_over(bytes, out_ptr, out_len);
            OK
        }
        Err(message) => {
            hand_over(message.into_bytes(), out_ptr, out_len);
            FAILED
        }
    }
}

/// `[{"kind","detail"}]` for the last conversion **on this thread**.
///
/// # Safety
/// The output pointers must be valid and writable.
#[no_mangle]
pub unsafe extern "C" fn libresvip_warnings_json(out_ptr: *mut *mut u8, out_len: *mut usize) -> i32 {
    let json = LAST_WARNINGS.with(|slot| {
        let stored = slot.borrow();
        if stored.is_empty() {
            // Before the first conversion there is nothing to report; answer
            // with an empty array rather than an empty string.
            "[]".to_owned()
        } else {
            stored.clone()
        }
    });
    hand_over(json.into_bytes(), out_ptr, out_len);
    OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::path::PathBuf;

    fn simple_project() -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("testdata")
            .join("simple.json");
        std::fs::read(path).expect("testdata/simple.json")
    }

    /// Frees a buffer the way a C caller would.
    unsafe fn take(pointer: *mut u8, len: usize) -> Vec<u8> {
        assert!(!pointer.is_null() || len == 0, "the library returned a null buffer");
        let bytes = std::slice::from_raw_parts(pointer, len).to_vec();
        libresvip_free(pointer, len);
        bytes
    }

    #[test]
    fn version_is_shipped() {
        let version = unsafe { CStr::from_ptr(libresvip_version()) }
            .to_str()
            .expect("utf-8");
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn converts_through_the_c_abi() {
        let input = simple_project();
        let input_format = CString::new("json").unwrap();
        let output_format = CString::new("ust").unwrap();
        let mut pointer: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        let status = unsafe {
            libresvip_convert(
                input.as_ptr(),
                input.len(),
                input_format.as_ptr(),
                output_format.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                &mut pointer,
                &mut len,
            )
        };
        assert_eq!(status, OK);
        let output = unsafe { take(pointer, len) };
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("Tempo"), "unexpected output: {text:?}");
    }

    #[test]
    fn errors_are_delivered_in_the_buffer() {
        let input_format = CString::new("nope").unwrap();
        let output_format = CString::new("ust").unwrap();
        let mut pointer: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        let status = unsafe {
            libresvip_convert(
                b"x".as_ptr(),
                1,
                input_format.as_ptr(),
                output_format.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                &mut pointer,
                &mut len,
            )
        };
        assert_eq!(status, FAILED);
        let message = unsafe { take(pointer, len) };
        assert!(String::from_utf8_lossy(&message).contains("unknown input format"));
    }

    #[test]
    fn formats_and_warnings_round_trip() {
        let mut pointer: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        assert_eq!(unsafe { libresvip_formats_json(&mut pointer, &mut len) }, OK);
        let formats = String::from_utf8(unsafe { take(pointer, len) }).expect("utf-8");
        assert!(formats.starts_with('[') && formats.contains("\"ustx\""));

        assert_eq!(unsafe { libresvip_warnings_json(&mut pointer, &mut len) }, OK);
        let warnings = String::from_utf8(unsafe { take(pointer, len) }).expect("utf-8");
        assert!(warnings.starts_with('['));
    }
}
