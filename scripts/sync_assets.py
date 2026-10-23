"""Copy runtime resources (translations) from the Python tree into `assets/`.

Usage (study12):

    E:\\miniconda\\envs\\study12\\python.exe scripts/sync_assets.py

The Rust CLI only needs the compiled gettext catalogs; the layout it expects is

    assets/res/locales/<lang>/LC_MESSAGES/core.mo
    assets/res/locales/<lang>/LC_MESSAGES/<plugin-id>.mo

`core.mo` is merged from `libresvip/res/locales`, every plugin contributes its
own `locales` directory (the same files `merge_translation` walks at runtime).
"""

from __future__ import annotations

import pathlib
import shutil

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
PY_ROOT = ROOT / "LibreSVIP" / "libresvip"
OUT_ROOT = pathlib.Path(__file__).resolve().parent.parent / "assets" / "res"
LANGS = ("de_DE", "ja_JP", "zh_CN", "zh_TW")


def copy_catalog(src_dir: pathlib.Path, dest: pathlib.Path) -> bool:
    if not src_dir.is_dir():
        return False
    copied = False
    for mo_file in src_dir.glob("*.mo"):
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(mo_file, dest)
        copied = True
    return copied


def main() -> None:
    for lang in LANGS:
        core_dest = OUT_ROOT / "locales" / lang / "LC_MESSAGES" / "core.mo"
        core_src = PY_ROOT / "res" / "locales" / lang / "LC_MESSAGES"
        copy_catalog(core_src, core_dest)

    for group in ("plugins", "middlewares"):
        for package in sorted((PY_ROOT / group).iterdir()):
            if not package.is_dir():
                continue
            for lang in LANGS:
                src = package / "locales" / lang / "LC_MESSAGES"
                dest = OUT_ROOT / "locales" / lang / "LC_MESSAGES" / f"{package.name}.mo"
                copy_catalog(src, dest)
    print(f"resources synced to {OUT_ROOT}")


if __name__ == "__main__":
    main()
