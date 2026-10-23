"""Generate the phoneme data assets consumed by `libresvip-core`.

Writes into `assets/phoneme/`:

* ``pinyin_normal.tsv``   — ``<char>\\t<tone-less pinyin>`` for every character of
  pypinyin's `pinyin_dict` (first pronunciation only, `Style.NORMAL`).
* ``phrases.tsv``         — ``<phrase>\\t<syllable> <syllable> ...`` for every
  entry of pypinyin's `phrases_dict`, again in `Style.NORMAL`.
* ``kana_romaji.json``    — wanakana's Hepburn kana→romaji trie with LibreSVIP's
  custom ``{"っ": "cl"}`` mapping merged in (exactly the tree
  `libresvip.core.lyric_phoneme.japanese.to_romaji` builds).
* ``ko_rr.tsv``           — the two `ko_pron` tables needed for
  ``romanise(word, system_index="rr")``: vowels and syllable-boundary rules.

The assets are embedded with `include_str!` and parsed lazily, so this script only
has to be re-run when pypinyin / wanakana / ko_pron change.

Usage:
    E:\\miniconda\\envs\\study12\\python.exe scripts/gen_phoneme_data.py
"""

from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets" / "phoneme"

from pypinyin.constants import PINYIN_DICT, PHRASES_DICT  # noqa: E402
from pypinyin.style._utils import replace_symbol_to_no_symbol  # noqa: E402

from ko_pron.data import boundary as ko_boundary  # noqa: E402
from ko_pron.data import vowels as ko_vowels  # noqa: E402
from wanakana.utils import get_kana_to_romaji_tree, merge_custom_mapping  # noqa: E402

# `libresvip.core.lyric_phoneme.japanese.to_romaji` overrides this one entry.
CUSTOM_ROMAJI_MAPPING = {"っ": "cl"}

# `ko_pron.ko_pron.system_lookup.index("rr")`
RR = 1


def write_pinyin() -> None:
    lines = []
    for codepoint in sorted(PINYIN_DICT):
        readings = PINYIN_DICT[codepoint].split(",")
        normal = replace_symbol_to_no_symbol(readings[0])
        character = chr(codepoint)
        if "\t" in normal or "\n" in normal:
            raise SystemExit(f"unexpected whitespace in pinyin for U+{codepoint:04X}")
        lines.append(f"{character}\t{normal}")
    path = ASSETS / "pinyin_normal.tsv"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{path.name}: {len(lines)} characters")


def write_phrases() -> None:
    lines = []
    for phrase in sorted(PHRASES_DICT):
        syllables = PHRASES_DICT[phrase]
        if len(syllables) != len(phrase):
            raise SystemExit(f"phrase/syllable count mismatch: {phrase}")
        if "\t" in phrase or "\n" in phrase:
            raise SystemExit(f"unexpected whitespace in phrase: {phrase!r}")
        values = []
        for alternatives in syllables:
            normal = replace_symbol_to_no_symbol(alternatives[0])
            if " " in normal or "\t" in normal:
                raise SystemExit(f"unexpected whitespace in phrase pinyin: {phrase}")
            values.append(normal)
        lines.append(f"{phrase}\t{' '.join(values)}")
    path = ASSETS / "phrases.tsv"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{path.name}: {len(lines)} phrases")


def write_kana() -> None:
    tree = get_kana_to_romaji_tree(romanisation="hepburn")
    tree = merge_custom_mapping(tree, CUSTOM_ROMAJI_MAPPING)

    def count(node: dict) -> tuple[int, int]:
        nodes = 1
        values = 1 if "" in node else 0
        for key, subtree in node.items():
            if key == "":
                continue
            child_nodes, child_values = count(subtree)
            nodes += child_nodes
            values += child_values
        return nodes, values

    nodes, values = count(tree)
    path = ASSETS / "kana_romaji.json"
    path.write_text(
        json.dumps(tree, ensure_ascii=False, separators=(",", ":"), sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"{path.name}: {nodes} nodes, {values} values")


def write_korean() -> None:
    lines = ["# ko_pron tables for system_index=\"rr\""]
    for vowel in sorted(ko_vowels):
        lines.append(f"V\t{vowel}\t{ko_vowels[vowel][RR]}")
    for key in sorted(ko_boundary):
        lines.append(f"B\t{key}\t{ko_boundary[key][RR]}")
    path = ASSETS / "ko_rr.tsv"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{path.name}: {len(ko_vowels)} vowels, {len(ko_boundary)} boundaries")


def main() -> None:
    ASSETS.mkdir(parents=True, exist_ok=True)
    write_pinyin()
    write_phrases()
    write_kana()
    write_korean()


if __name__ == "__main__":
    sys.exit(main())
