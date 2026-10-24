//! File system access for the format plugins.
//!
//! Every plugin goes through this module instead of `std::fs` so the same code
//! can run on `wasm32`, where there is no file system: there the paths are
//! served from an in-memory table ([`crate::memory_fs`]). On every other target
//! these are plain re-exports of `std::fs`, so native builds keep the exact same
//! behaviour and cost.

#[cfg(not(target_arch = "wasm32"))]
pub use std::fs::{create_dir_all, read, read_to_string, remove_dir_all, remove_file, write};

#[cfg(target_arch = "wasm32")]
pub use crate::memory_fs::{
    create_dir_all, read, read_shared, read_to_string, remove_dir_all, remove_file, write,
};

#[cfg(not(target_arch = "wasm32"))]
/// Native twin of the wasm `read_shared`; shares nothing but keeps one API.
pub fn read_shared(path: impl AsRef<std::path::Path>) -> std::io::Result<std::sync::Arc<[u8]>> {
    Ok(std::sync::Arc::from(std::fs::read(path)?))
}
