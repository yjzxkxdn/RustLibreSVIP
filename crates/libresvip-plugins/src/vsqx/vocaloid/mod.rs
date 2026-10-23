//! Shared Vocaloid model helpers for the `vsqx`, `vsq` and `xvsq` plugins.
//!
//! The Python originals live in `libresvip.model.vocaloid` and are used by all
//! three formats, so the port keeps them in a single module owned by `vsqx`
//! (`vsq` and `xvsq` import them from here).

pub mod controllers;
pub mod pitch;

pub use controllers::*;
pub use pitch::*;
