"""Patch the UtaFormatix3 Kotlin/JS library so it can run under Node.

The `browserProductionLibraryDistribution` output is an ES module, but the
Kotlin compiler emits `require('./format_templates/template.svp')` for the
bundled text resources and `require('midi-file')` for the MIDI package.
Node refuses `require` inside an ESM that also uses top-level await, so the
calls are rewritten here:

  * templates -> inline string constants (the JSON one stays a parsed array,
    because the Kotlin side asks for an Array, not for a string)
  * midi-file -> a normal ESM import

Only the distribution file is touched; the Kotlin sources stay untouched.
"""

from __future__ import annotations

import json
import pathlib
import re
import shutil

REF = pathlib.Path(r"E:\RUST_LIBRESVIP\.refs\utaformatix3")
DIST = REF / "core" / "build" / "dist" / "js" / "productionLibrary"
TEMPLATES = REF / "core" / "src" / "main" / "resources" / "format_templates"
OUT_DIR = REF / "build" / "js" / "uf3dist"
OUT_NAME = "utaformatix-core.patched.mjs"


def main() -> int:
    source = DIST / "utaformatix-core.mjs"
    text = source.read_text(encoding="utf-8")

    resources: dict[str, str] = {}
    for path in sorted(TEMPLATES.iterdir()):
        if not path.is_file():
            continue
        key = f"./format_templates/{path.name}"
        value = path.read_text(encoding="utf-8")
        if path.suffix == ".json":
            resources[key] = "__array__" + json.dumps(json.loads(value), ensure_ascii=False)
        else:
            resources[key] = json.dumps(value, ensure_ascii=False)

    preamble = ['import __uf3midi from "midi-file";', "const __uf3_res = {};"]
    for key, value in resources.items():
        if value.startswith("__array__"):
            preamble.append(f"__uf3_res[{json.dumps(key)}] = {value[9:]};")
        else:
            preamble.append(f"__uf3_res[{json.dumps(key)}] = {{ default: {value} }};")

    calls = sorted(set(re.findall(r"require\((['\"][^'\"]+['\"])\)", text)))
    replaced = 0
    for literal in calls:
        name = literal[1:-1]
        if name == "midi-file":
            text = text.replace(f"require({literal})", "__uf3midi")
            replaced += 1
        elif name in resources:
            text = text.replace(f"require({literal})", f"__uf3_res[{json.dumps(name)}]")
            replaced += 1
        else:
            raise SystemExit(f"unexpected require: {name}")

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for item in DIST.iterdir():
        target = OUT_DIR / item.name
        if item.is_dir():
            shutil.copytree(item, target, dirs_exist_ok=True)
        elif item.name != "utaformatix-core.mjs":
            shutil.copy2(item, target)
    out = OUT_DIR / OUT_NAME
    out.write_text("\n".join(preamble) + "\n" + text, encoding="utf-8")
    print(f"patched {len(calls)} require() calls, wrote {out} ({out.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
