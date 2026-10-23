//! In-memory replacement for the handful of `std::fs` calls the plugins make,
//! used when the crate is compiled for `wasm32` (browsers, wasm servers).
//!
//! The host registers the input file under a virtual path, calls the converter,
//! and reads the generated file back out. Directories are implicit: they only
//! exist as path prefixes, so `create_dir_all` is a no-op and `remove_dir_all`
//! drops every file below the prefix.
//!
//! This module is only compiled on `wasm32`; native builds use [`crate::io`]'s
//! `std::fs` re-exports instead.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

fn files() -> MutexGuard<'static, BTreeMap<PathBuf, Vec<u8>>> {
    static FILES: OnceLock<Mutex<BTreeMap<PathBuf, Vec<u8>>>> = OnceLock::new();
    FILES
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Lexical `.`/`..` normalisation, so `a/../b` and `b` name the same file.
fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn missing(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{}: no such file in the in-memory file system", path.display()),
    )
}

pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path = normalize(path.as_ref());
    files().get(&path).cloned().ok_or_else(|| missing(&path))
}

pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let path = path.as_ref();
    let bytes = read(path)?;
    String::from_utf8(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", path.display()),
        )
    })
}

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let path = normalize(path.as_ref());
    files().insert(path, contents.as_ref().to_vec());
    Ok(())
}

pub fn create_dir_all(_path: impl AsRef<Path>) -> io::Result<()> {
    Ok(())
}

pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    let path = normalize(path.as_ref());
    if files().remove(&path).is_some() {
        Ok(())
    } else {
        Err(missing(&path))
    }
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let dir = normalize(path.as_ref());
    files().retain(|candidate, _| !candidate.starts_with(&dir));
    Ok(())
}
