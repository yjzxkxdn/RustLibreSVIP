//! Placeholder plugin implementation used until a format is ported.
//!
//! It carries the real metadata so `plugin list` / `plugin detail` are already
//! complete, but refuses to convert.

use std::path::Path;

use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::Options;
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

pub struct StubPlugin {
    pub info: &'static PluginInfo,
    pub version: &'static str,
}

impl SvsPlugin for StubPlugin {
    fn info(&self) -> &'static PluginInfo {
        self.info
    }

    fn version(&self) -> &'static str {
        self.version
    }

    fn readable(&self) -> bool {
        // `WriteOnlyConverterMixin` formats cannot be imported.
        !matches!(self.info.id, "ass" | "lrc" | "srt" | "svg")
    }

    fn writable(&self) -> bool {
        // `ReadOnlyConverterMixin` formats cannot be exported.
        !matches!(self.info.id, "vshp")
    }

    fn load(&self, _path: &Path, _options: &Options) -> Result<Project> {
        Err(LibreSvipError::msg(format!(
            "plugin {} is not implemented yet",
            self.info.id
        )))
    }

    fn dump(&self, _path: &Path, _project: &Project, _options: &Options) -> Result<()> {
        Err(LibreSvipError::msg(format!(
            "plugin {} is not implemented yet",
            self.info.id
        )))
    }
}
