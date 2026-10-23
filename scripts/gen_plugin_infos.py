"""Generate `crates/libresvip-plugins/src/plugin_infos.rs` from the Python sources.

Run with the study12 interpreter:

    E:\\miniconda\\envs\\study12\\python.exe scripts/gen_plugin_infos.py

The script reads every `*.yapsy-plugin` file exactly like
`libresvip.extension.meta_info` does, and asks the live plugin registry for the
`_alias_` / `_version_` values, so the generated Rust constants cannot drift
from the Python implementation.
"""

from __future__ import annotations

import pathlib
import sys

PY_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent / "LibreSVIP"
if str(PY_ROOT) not in sys.path:
    sys.path.insert(0, str(PY_ROOT))

from configparser import RawConfigParser  # noqa: E402

from libresvip.extension.manager import plugin_manager  # noqa: E402

OUT = (
    pathlib.Path(__file__).resolve().parent.parent
    / "crates"
    / "libresvip-plugins"
    / "src"
    / "plugin_infos.rs"
)


def rust_string(value: str) -> str:
    parts = []
    for ch in value:
        if ch == "\\":
            parts.append("\\\\")
        elif ch == '"':
            parts.append('\\"')
        elif ch == "\n":
            parts.append("\\n")
        elif ch == "\r":
            parts.append("\\r")
        elif ch == "\t":
            parts.append("\\t")
        else:
            parts.append(ch)
    return '"' + "".join(parts) + '"'


def const_name(plugin_id: str) -> str:
    return "INFO_" + plugin_id.upper().replace("-", "_")


def main() -> None:
    plugins = plugin_manager.plugins["svs"]
    lines = [
        "//! Plugin metadata, generated from `libresvip/plugins/*/*.yapsy-plugin`.",
        "//!",
        "//! Regenerate with `python scripts/gen_plugin_infos.py` (see module docs);",
        "//! do not edit by hand.",
        "",
        "use libresvip_core::plugin::PluginInfo;",
        "",
    ]
    for plugin_id in sorted(plugins):
        plugin = plugins[plugin_id]
        package = pathlib.Path(plugin.__module__.replace(".", "/")).parent
        if not package.is_absolute():
            package = PY_ROOT / package
        meta_files = list(package.glob("*.yapsy-plugin"))
        if not meta_files:
            raise SystemExit(f"no yapsy metadata for {plugin_id}")
        parser = RawConfigParser()
        parser.read_string(meta_files[0].read_text(encoding="utf-8"))
        core = parser["Core"]
        doc = parser["Documentation"]
        description = (
            doc.get("Description", "").encode("raw_unicode_escape").decode("unicode_escape")
        )
        raw_suffixes = doc["Suffix"]
        suffixes = [
            part.strip().removeprefix(".") for part in raw_suffixes.split(",")
        ]
        suffix_literals = ", ".join(rust_string(suffix) for suffix in suffixes)
        lines.extend(
            [
                f"pub const {const_name(plugin_id)}: PluginInfo = PluginInfo {{",
                f"    id: {rust_string(plugin_id)},",
                f"    name: {rust_string(core['Name'])},",
                f"    author: {rust_string(doc.get('Author', 'Unknown Author'))},",
                f"    website: {rust_string(doc.get('Website', ''))},",
                f"    file_format: {rust_string(doc['Format'])},",
                f"    description: {rust_string(description)},",
                f"    suffixes: &[{suffix_literals}],",
                "    icon_base64: None,",
                "};",
                f"pub const {const_name(plugin_id)}_VERSION: &str = {rust_string(plugin._version_)};",
                "",
            ]
        )
    OUT.write_text("\n".join(lines), encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(plugins)} plugins)")


if __name__ == "__main__":
    main()
