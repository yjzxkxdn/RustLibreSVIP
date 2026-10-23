//! `vshp` plugin — port of `libresvip.plugins.vshp` (VocalShifter).
//!
//! The Python converter is a `ReadOnlyConverterMixin`: VocalShifter projects can
//! be imported but not written, so this plugin is [`SvsPlugin::readable`] only
//! and [`SvsPlugin::writable`] returns `false`.

pub mod model;
pub mod parser;
pub mod utils;

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct VshpPlugin;

pub static PLUGIN: VshpPlugin = VshpPlugin;

impl SvsPlugin for VshpPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_VSHP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_VSHP_VERSION
    }

    /// `InputOptions` of the plugin, in declaration order.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::bool(
                "wave_to_singing",
                "Convert wave pattern to singing pattern",
                true,
            ),
            OptionFieldDef::bool("use_edited_pitch", "Use edited pitch curve", true),
            OptionFieldDef::bool("use_edited_dynamics", "Use edited dynamics curve", true),
            OptionFieldDef::bool("import_dynamics", "Import dynamics curve", false),
            OptionFieldDef::bool("import_formant", "Import formant curve", false),
            OptionFieldDef::bool("import_breath", "Import breath curve", false),
        ]
    }

    /// `ReadOnlyConverterMixin` — there is no generator.
    fn writable(&self) -> bool {
        false
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let project_data = model::parse_project(&bytes)?;
        let input_options = parser::InputOptions {
            wave_to_singing: options.bool("wave_to_singing", true),
            use_edited_pitch: options.bool("use_edited_pitch", true),
            use_edited_dynamics: options.bool("use_edited_dynamics", true),
            import_dynamics: options.bool("import_dynamics", false),
            import_formant: options.bool("import_formant", false),
            import_breath: options.bool("import_breath", false),
        };
        parser::VocalShifterParser::new(input_options).parse_project(&project_data)
    }
}
