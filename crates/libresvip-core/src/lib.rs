//! Core data models and conversion infrastructure for the Rust port of LibreSVIP.
//!
//! The layout mirrors the Python package `libresvip`:
//!
//! * [`model`] — internal project models (`Project`, `Track`, `Note`, pitch curves, …)
//! * [`core`] — tick math, time synchronisation, settings, warnings, g2p
//! * [`plugin`] — format plugin trait and registry primitives
//! * [`options`] — schema-driven option definitions (the Rust equivalent of
//!   introspecting pydantic `model_fields`)
//! * [`utils`] — text/binary/xml/yaml helpers plus gettext translations

pub mod capabilities;
pub mod compression;
pub mod core;
pub mod error;
pub mod io;
#[cfg(target_arch = "wasm32")]
pub mod memory_fs;
pub mod model;
pub mod options;
pub mod platform;
pub mod plugin;
pub mod semantics;
pub mod utils;

pub use error::{LibreSvipError, Result};
pub use options::{EnumVariant, OptionFieldDef, OptionKind, OptionValue, Options};
pub use plugin::{Middleware, PluginInfo, SvsPlugin};

/// Version of the Python implementation this port tracks.
pub const VERSION: &str = "2.9.2";

/// The UtaFormatix3 revision whose design this port follows (capability matrix,
/// structured warnings, pitch semantics, Japanese lyrics, phoneme mapping and
/// version negotiation). Only the design was adopted; no code was copied.
pub const UTAFORMATIX3_REVISION: &str = "f3c8335";
