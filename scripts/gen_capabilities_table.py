"""Inject the format-capabilities table into README.md.

Reads `ALL_CAPABILITIES` from
`crates/libresvip-plugins/src/capabilities.rs` (the single source of truth,
also served by `libresvip-cli plugin capabilities --json`) and replaces the
block between the FORMAT-CAPABILITIES markers.

Usage:
    python scripts/gen_capabilities_table.py
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
README = ROOT / "README.md"
CAPABILITIES = ROOT / "crates" / "libresvip-plugins" / "src" / "capabilities.rs"

START = "<!-- FORMAT-CAPABILITIES-START -->"
END = "<!-- FORMAT-CAPABILITIES-END -->"


def main() -> int:
    source = CAPABILITIES.read_text(encoding="utf-8")
    body = source[source.index("pub const ALL_CAPABILITIES") : source.index("#[cfg(test)]")]
    entries: list[tuple[str, list[str], str | None]] = []
    for match in re.finditer(r'\(\s*"(\w+)"\s*,\s*caps!\s*\[(.*?)\]\s*,?\s*\),', body, re.S):
        plugin_id, feature_blob = match.group(1), match.group(2)
        features = [f.strip() for f in feature_blob.replace("\n", " ").split(",") if f.strip()]
        single = "single" in feature_blob
        entries.append((plugin_id, features, 1 if single else None))

    if len(entries) < 40:
        raise SystemExit(f"only parsed {len(entries)} capability entries")

    head = (
        "| 格式 | 音高 | 音量 | 气声 | 性别 | 强度 | 音素 | 颤音 | 伴奏轨 | 单轨 |\n"
        "| --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: |\n"
    )
    rows = []
    for plugin_id, features, single in entries:
        lowered = {feature.lower() for feature in features}

        def has(key: str) -> str:
            return "✓" if key in lowered else ""

        cells = [
            f"`.{plugin_id}`",
            *map(has, ("pitch", "volume", "breath", "gender", "strength")),
            has("phonemes"),
            has("vibrato"),
            has("instrumental_tracks"),
            "✓" if single else "",
        ]
        rows.append("| " + " | ".join(cells) + " |")
    legend = (
        "\n音高/音量/气声/性别/强度 = 五条参数曲线（导入或导出至少一侧支持）；"
        "单轨 = 一个输出文件只容纳一条轨道。\n"
    )

    readme = README.read_text(encoding="utf-8")
    start = readme.index(START) + len(START)
    end = readme.index(END)
    updated = readme[:start] + "\n" + head + "\n".join(rows) + legend + readme[end:]
    README.write_text(updated, encoding="utf-8", newline="")
    print(f"README table: {len(entries)} formats")
    return 0


if __name__ == "__main__":
    sys.exit(main())
