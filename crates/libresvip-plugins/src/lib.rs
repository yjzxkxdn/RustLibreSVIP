//! Format conversion plugins for the Rust port of LibreSVIP.
//!
//! The Python implementation discovers plugins dynamically (yapsy). Rust has no
//! such reflection, so plugins are compiled in and registered in a static list.
//! Metadata is generated from the real `.yapsy-plugin` files by
//! `scripts/gen_plugin_infos.py`, and identifiers/suffix lookup mirrors
//! `libresvip.extension.manager`.
//!
//! One module per format, named after the plugin identifier. Each module exposes
//! a `PLUGIN` static implementing [`libresvip_core::plugin::SvsPlugin`].

pub mod plugin_infos;
pub mod interval_dict;
pub mod stub;

// Implemented plugins (one module per format identifier).
pub mod ace;
pub mod acep;
pub mod aisp;
pub mod ass;
pub mod ccs;
pub mod ds;
pub mod dspx;
pub mod dv;
pub mod json;
pub mod lrc;
pub mod mid;
pub mod mtp;
pub mod musicxml;
pub mod nn;
pub mod ppsf;
pub mod ps_project;
pub mod s5p;
pub mod srt;
pub mod svg;
pub mod svip;
pub mod svip3;
pub mod svp;
pub mod tlp;
pub mod tlpx;
pub mod tsmsln;
pub mod tssln;
pub mod ufdata;
pub mod ust;
pub mod ustx;
pub mod vfp;
pub mod vog;
pub mod vpr;
pub mod vshp;
pub mod vspx;
pub mod vsq;
pub mod vsqx;
pub mod vvproj;
pub mod vxf;
pub mod xvsq;
pub mod y77;

use std::sync::OnceLock;

use libresvip_core::core::config::read_settings;
use libresvip_core::plugin::SvsPlugin;

pub use stub::StubPlugin;

fn implemented_instances() -> Vec<&'static dyn SvsPlugin> {
    vec![
        &ace::PLUGIN as &'static dyn SvsPlugin,
        &acep::PLUGIN as &'static dyn SvsPlugin,
        &aisp::PLUGIN as &'static dyn SvsPlugin,
        &ass::PLUGIN as &'static dyn SvsPlugin,
        &ccs::PLUGIN as &'static dyn SvsPlugin,
        &ds::PLUGIN as &'static dyn SvsPlugin,
        &dspx::PLUGIN as &'static dyn SvsPlugin,
        &dv::PLUGIN as &'static dyn SvsPlugin,
        &json::PLUGIN as &'static dyn SvsPlugin,
        &lrc::PLUGIN as &'static dyn SvsPlugin,
        &mid::PLUGIN as &'static dyn SvsPlugin,
        &mtp::PLUGIN as &'static dyn SvsPlugin,
        &musicxml::PLUGIN as &'static dyn SvsPlugin,
        &nn::PLUGIN as &'static dyn SvsPlugin,
        &ppsf::PLUGIN as &'static dyn SvsPlugin,
        &ps_project::PLUGIN as &'static dyn SvsPlugin,
        &s5p::PLUGIN as &'static dyn SvsPlugin,
        &srt::PLUGIN as &'static dyn SvsPlugin,
        &svg::PLUGIN as &'static dyn SvsPlugin,
        &svip::PLUGIN as &'static dyn SvsPlugin,
        &svip3::PLUGIN as &'static dyn SvsPlugin,
        &svp::PLUGIN as &'static dyn SvsPlugin,
        &tlp::PLUGIN as &'static dyn SvsPlugin,
        &tlpx::PLUGIN as &'static dyn SvsPlugin,
        &tsmsln::PLUGIN as &'static dyn SvsPlugin,
        &tssln::PLUGIN as &'static dyn SvsPlugin,
        &ufdata::PLUGIN as &'static dyn SvsPlugin,
        &ust::PLUGIN as &'static dyn SvsPlugin,
        &ustx::PLUGIN as &'static dyn SvsPlugin,
        &vfp::PLUGIN as &'static dyn SvsPlugin,
        &vog::PLUGIN as &'static dyn SvsPlugin,
        &vpr::PLUGIN as &'static dyn SvsPlugin,
        &vshp::PLUGIN as &'static dyn SvsPlugin,
        &vspx::PLUGIN as &'static dyn SvsPlugin,
        &vsq::PLUGIN as &'static dyn SvsPlugin,
        &vsqx::PLUGIN as &'static dyn SvsPlugin,
        &vvproj::PLUGIN as &'static dyn SvsPlugin,
        &vxf::PLUGIN as &'static dyn SvsPlugin,
        &xvsq::PLUGIN as &'static dyn SvsPlugin,
        &y77::PLUGIN as &'static dyn SvsPlugin,
    ]
}

/// Every registered plugin, sorted by identifier (the order yapsy yields).
pub fn all_plugins() -> &'static [&'static dyn SvsPlugin] {
    static REGISTRY: OnceLock<Vec<&'static dyn SvsPlugin>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut list = implemented_instances();
        list.sort_by_key(|plugin| plugin.info().id);
        list
    })
}

/// Plugins enabled by the current settings (`blacklist=settings.disabled_plugins`).
pub fn enabled_plugins() -> Vec<&'static dyn SvsPlugin> {
    let disabled = read_settings(|settings| settings.disabled_plugins.clone());
    all_plugins()
        .iter()
        .copied()
        .filter(|plugin| !disabled.iter().any(|item| item == plugin.info().id))
        .collect()
}

/// Plugin identifiers, used to build the translation chain.
pub fn plugin_ids() -> Vec<&'static str> {
    all_plugins()
        .iter()
        .map(|plugin| plugin.info().id)
        .collect()
}

/// `get_svs_plugin_by_suffix`.
pub fn plugin_by_suffix(suffix: &str) -> Option<&'static dyn SvsPlugin> {
    suffix_map().get(suffix).copied()
}

/// `get_svs_plugin_by_value` — identifier first, then suffix.
pub fn plugin_by_value(value: &str) -> Option<&'static dyn SvsPlugin> {
    enabled_plugins()
        .into_iter()
        .find(|plugin| plugin.info().id == value)
        .or_else(|| plugin_by_suffix(value))
}

/// Suffix → plugin map, logged for duplicates like `_build_svs_suffix_map`.
fn suffix_map() -> &'static std::collections::HashMap<&'static str, &'static dyn SvsPlugin> {
    static MAP: OnceLock<std::collections::HashMap<&'static str, &'static dyn SvsPlugin>> =
        OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for plugin in enabled_plugins() {
            for suffix in plugin.info().suffixes {
                if let Some(previous) = map.insert(*suffix, plugin) {
                    let previous: &dyn SvsPlugin = previous;
                    eprintln!(
                        "WARNING: Duplicate suffix '{}' declared by plugins '{}' and '{}'",
                        suffix,
                        previous.info().id,
                        plugin.info().id
                    );
                }
            }
        }
        map
    })
}

/// `get_supported_suffixes` — sorted suffixes of enabled plugins.
pub fn supported_suffixes() -> Vec<&'static str> {
    let mut suffixes: Vec<&'static str> = suffix_map().keys().copied().collect();
    suffixes.sort_unstable();
    suffixes
}

/// `get_output_suffixes` — sorted primary suffixes.
pub fn output_suffixes() -> Vec<&'static str> {
    let mut suffixes: Vec<&'static str> = enabled_plugins()
        .into_iter()
        .map(|plugin| plugin.info().suffix())
        .filter(|suffix| !suffix.is_empty())
        .collect();
    suffixes.sort_unstable();
    suffixes
}

/// `get_duplicate_suffixes` — suffix → plugin ids, for suffixes claimed twice.
pub fn duplicate_suffixes() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut mapping: std::collections::BTreeMap<&'static str, Vec<&'static str>> =
        std::collections::BTreeMap::new();
    for plugin in all_plugins() {
        for suffix in plugin.info().suffixes {
            mapping.entry(*suffix).or_default().push(plugin.info().id);
        }
    }
    mapping
        .into_iter()
        .filter(|(_, plugins)| plugins.len() > 1)
        .collect()
}

/// `get_svs_plugin_suffixes`.
pub fn plugin_suffixes(value: &str) -> &'static [&'static str] {
    plugin_by_value(value)
        .map(|plugin| plugin.info().suffixes)
        .unwrap_or(&[])
}
