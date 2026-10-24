"""(Re)compress the embedded phoneme dictionaries.

The big tables (pypinyin phrase/character TSVs, SynthV phoneme dictionary)
are `include_bytes!`-ed as zlib streams to keep the wasm data section small;
`libresvip-core` inflates them lazily on first use.

Run from the workspace root after regenerating the raw assets:

    python scripts/compress_phoneme_assets.py
"""

import pathlib
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent

ASSETS = [
    "assets/phoneme/pinyin_normal.tsv",
    "assets/phoneme/phrases.tsv",
    "assets/phoneme/jyutping.tsv",
    "assets/phoneme/ko_roman.tsv",
    "crates/libresvip-plugins/src/svp/phoneme_dictionary.json",
]


def main() -> None:
    for relative in ASSETS:
        source = ROOT / relative
        packed = zlib.compress(source.read_bytes(), 9)
        target = source.with_name(source.name + ".zlib")
        target.write_bytes(packed)
        print(f"{relative}: {source.stat().st_size:,} -> {len(packed):,} bytes")


if __name__ == "__main__":
    main()
