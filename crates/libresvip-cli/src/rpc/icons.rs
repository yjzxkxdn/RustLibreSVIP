//! Icon payloads for `PluginInfo.icon_base64`.
//!
//! Python reads `Documentation/IconBase64` from each `<plugin>.yapsy-plugin`
//! file (`libresvip.extension.meta_info`). The Rust plugin metadata generated
//! by `scripts/gen_plugin_infos.py` deliberately omits those large strings, so
//! the table is embedded here, generated once from the reference metadata.
//! Keyed by plugin identifier; input and output use the same icon.
//!
//! To regenerate, dump `PluginInfos` from the Python reference (`INPUT` and
//! `OUTPUT`; `scripts/check_rpc.py` already speaks that protocol) and keep
//! `identifier -> icon_base64` for every entry with a non-empty icon.

use std::collections::HashMap;
use std::sync::OnceLock;

const ICONS_JSON: &str = include_str!("icons.json");

fn icons() -> &'static HashMap<String, String> {
    static ICONS: OnceLock<HashMap<String, String>> = OnceLock::new();
    ICONS.get_or_init(|| serde_json::from_str(ICONS_JSON).unwrap_or_default())
}

/// `plugin.info.icon_base64 or ""`.
pub fn icon_base64(identifier: &str) -> String {
    icons().get(identifier).cloned().unwrap_or_default()
}
