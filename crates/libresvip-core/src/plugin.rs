//! Format plugin / middleware traits — the Rust counterpart of
//! `libresvip.extension.base` and `libresvip.extension.meta_info`.

use std::path::Path;

use crate::error::{LibreSvipError, Result};
use crate::model::base::Project;
use crate::options::{OptionFieldDef, Options};

/// Static metadata parsed from a `<plugin>.yapsy-plugin` file in Python.
#[derive(Clone, Copy, Debug)]
pub struct PluginInfo {
    /// Plugin identifier (`_alias_`), e.g. `"svp"`.
    pub id: &'static str,
    pub name: &'static str,
    pub author: &'static str,
    pub website: &'static str,
    /// `Documentation/Format`.
    pub file_format: &'static str,
    pub description: &'static str,
    /// `Documentation/Suffix`, split on commas with dots removed.
    pub suffixes: &'static [&'static str],
    pub icon_base64: Option<&'static str>,
}

impl PluginInfo {
    /// Primary suffix (`suffixes[0]`).
    pub fn suffix(&self) -> &'static str {
        self.suffixes.first().copied().unwrap_or("")
    }

    /// `"; ".join(f"*.{s}")` as used by the CLI.
    pub fn suffixes_display(&self) -> String {
        self.suffixes
            .iter()
            .map(|suffix| format!("*.{suffix}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// A project format converter (`SVSConverter`).
///
/// The `ReadOnlyConverterMixin` / `WriteOnlyConverterMixin` distinction is
/// expressed by [`SvsPlugin::writable`] and [`SvsPlugin::readable`].
pub trait SvsPlugin: Sync {
    fn info(&self) -> &'static PluginInfo;

    /// `_version_` — shown by `plugin list` / `plugin detail`.
    fn version(&self) -> &'static str {
        "0.0.0"
    }

    /// `input_option_cls.model_fields`, in declaration order.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    /// `output_option_cls.model_fields`, in declaration order.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    fn readable(&self) -> bool {
        true
    }

    fn writable(&self) -> bool {
        true
    }

    fn load(&self, _path: &Path, _options: &Options) -> Result<Project> {
        Err(LibreSvipError::NotImplemented)
    }

    fn dump(&self, _path: &Path, _project: &Project, _options: &Options) -> Result<()> {
        Err(LibreSvipError::NotImplemented)
    }
}

/// A project middleware (`Middleware`).
pub trait Middleware: Sync {
    fn info(&self) -> &'static PluginInfo;

    /// `process_option_cls.model_fields`, in declaration order.
    fn process_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    fn process(&self, project: Project, options: &Options) -> Result<Project>;
}
