"""Generate golden fixtures from the Python reference implementation.

For every format plugin this writes, under `testdata/golden/<plugin-id>/`:

* `dump.<suffix>` — the plugin output for `testdata/simple.json` using default
  output options (written twice to detect non-deterministic formats)
* `project.json` — the internal project model produced by loading that dump
* `meta.json` — status flags (`dumped`, `loaded`, `nondeterministic`, errors)

The Rust integration test `crates/libresvip-plugins/tests/golden.rs` consumes
these files: it must load `dump.<suffix>` into the same project model and
re-produce `dump.<suffix>` byte-for-byte (unless the format is marked
non-deterministic).

Usage (study12):

    E:\\miniconda\\envs\\study12\\python.exe scripts/gen_golden.py [--only id]
"""

from __future__ import annotations

import argparse
import json
import time
import pathlib
import shutil
import sys
import traceback

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
PY_ROOT = ROOT / "LibreSVIP"
if str(PY_ROOT) not in sys.path:
    sys.path.insert(0, str(PY_ROOT))

from libresvip.extension.manager import plugin_manager  # noqa: E402
from libresvip.model.base import Project  # noqa: E402
from pydantic_core import PydanticUndefined  # noqa: E402
from pydantic_extra_types.color import Color  # noqa: E402

TESTDATA = pathlib.Path(__file__).resolve().parent.parent / "testdata"
GOLDEN = TESTDATA / "golden"

WRITE_ONLY = {"ass", "lrc", "srt", "svg"}

# Formats that cannot handle instrumental tracks (or need a colour) use the
# singing-only fixture.
FIXTURE_OVERRIDE = {
    "svg": "simple_singing.json",
    "xvsq": "simple_singing.json",
}
READ_ONLY = {"vshp"}


def default_options(option_cls) -> dict:
    """Materialise option defaults the way the CLI prompts would.

    pydantic does not validate defaults, so `Color` fields hold plain strings
    until they are passed back in; wrap them so plugins see real Color objects.
    """
    if option_cls is None:
        return {}
    values = {}
    for name, field in option_cls.model_fields.items():
        if field.default is PydanticUndefined:
            continue
        value = field.default
        if field.annotation is Color and isinstance(value, str):
            value = Color(value)
        values[name] = value
    return values


def load_simple(name: str = "simple.json") -> Project:
    return Project.model_validate_json((TESTDATA / name).read_bytes().decode("utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--only", default=None)
    args = parser.parse_args()

    plugins = plugin_manager.plugins["svs"]
    GOLDEN.mkdir(parents=True, exist_ok=True)
    summary: dict[str, dict[str, object]] = {}

    for plugin_id in sorted(plugins):
        if args.only and args.only != plugin_id:
            continue
        plugin = plugins[plugin_id]
        # Reload per plugin: the Python plugins mutate the project in place.
        project = load_simple(FIXTURE_OVERRIDE.get(plugin_id, "simple.json"))
        info = plugin.info
        suffix = info.suffix
        folder = GOLDEN / plugin_id
        if folder.exists():
            shutil.rmtree(folder)
        folder.mkdir(parents=True)
        meta: dict[str, object] = {
            "id": plugin_id,
            "suffix": suffix,
            "fixture": FIXTURE_OVERRIDE.get(plugin_id, "simple.json"),
            "dumped": False,
            "loaded": False,
            "nondeterministic": False,
            "notes": [],
        }
        first_path = folder / f"dump.{suffix}"
        second_path = folder / f"second.{suffix}"
        try:
            plugin.dump(first_path, project, default_options(plugin.output_option_cls))
            meta["dumped"] = True
        except Exception as error:  # noqa: BLE001
            meta["notes"].append(f"dump failed: {error!r}")
            traceback.print_exc(limit=2)

        if meta["dumped"]:
            try:
                # Sleep so formats that embed a millisecond timestamp are detected
                # as non-deterministic.
                time.sleep(2.2)  # ZIP DOS timestamps have 2 s granularity
                plugin.dump(second_path, project, default_options(plugin.output_option_cls))
                if second_path.read_bytes() != first_path.read_bytes():
                    meta["nondeterministic"] = True
                second_path.unlink(missing_ok=True)
            except Exception as error:  # noqa: BLE001
                meta["notes"].append(f"second dump failed: {error!r}")

            if plugin_id not in WRITE_ONLY:
                try:
                    loaded = plugin.load(first_path, default_options(plugin.input_option_cls))
                    (folder / "project.json").write_bytes(
                        json.dumps(
                            loaded.model_dump(mode="json", by_alias=True),
                            ensure_ascii=False,
                            indent=2,
                        ).encode("utf-8")
                    )
                    meta["loaded"] = True
                except Exception as error:  # noqa: BLE001
                    meta["notes"].append(f"load failed: {error!r}")
                    traceback.print_exc(limit=2)

        (folder / "meta.json").write_text(
            json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        summary[plugin_id] = meta
        status = "ok" if meta["dumped"] and (plugin_id in WRITE_ONLY or meta["loaded"]) else "PARTIAL"
        print(f"{plugin_id:12s} {status:8s} dump={meta['dumped']} load={meta['loaded']} notes={len(meta['notes'])}")

    (GOLDEN / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    complete = sum(1 for meta in summary.values() if meta["dumped"] and (meta["id"] in WRITE_ONLY or meta["loaded"]))
    print(f"\n{complete}/{len(summary)} plugins have complete goldens")
    return 0


if __name__ == "__main__":
    sys.exit(main())
