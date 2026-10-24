"""Generate the phoneme data assets consumed by `libresvip-core`.

Writes into `assets/phoneme/`:

* ``pinyin_normal.tsv``   — ``<char>\\t<tone-less pinyin>`` for every character of
  pypinyin's `pinyin_dict` (first pronunciation only, `Style.NORMAL`).
* ``phrases.tsv``         — ``<phrase>\\t<syllable> <syllable> ...`` for every
  entry of pypinyin's `phrases_dict`, again in `Style.NORMAL`.
* ``kana_romaji.json``    — wanakana's Hepburn kana→romaji trie with LibreSVIP's
  custom ``{"っ": "cl"}`` mapping merged in (exactly the tree
  `libresvip.core.lyric_phoneme.japanese.to_romaji` builds).
* ``ko_roman.tsv``        — the two `ko_pron` tables (vowels, syllable
  boundaries) with one column each for the `rr`, `rrr` and `mr` systems.
* ``jyutping.tsv``        — ``<char>\\t<jyutping>`` from the `jyutping` package
  dictionary (sorted-first reading; Python's set.pop pick is hash-order
  dependent, so a deterministic one is used instead).

Also writes ``testdata/g2p_battery.tsv``, the Python-generated expectations
the Rust g2p parity test runs against (jyutping + korean rr/rrr/mr).

The assets are embedded with `include_str!` and parsed lazily, so this script only
has to be re-run when pypinyin / wanakana / ko_pron / jyutping change.

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

from jyutping import dictionary as jyutping_dictionary  # noqa: E402
from ko_pron.data import boundary as ko_boundary  # noqa: E402
from ko_pron.data import vowels as ko_vowels  # noqa: E402
from wanakana.utils import get_kana_to_romaji_tree, merge_custom_mapping  # noqa: E402

# `libresvip.core.lyric_phoneme.japanese.to_romaji` overrides this one entry.
CUSTOM_ROMAJI_MAPPING = {"っ": "cl"}

# `ko_pron.ko_pron.system_lookup.index(...)`
RR, RRR, MR = 1, 2, 3

# Deterministic word batteries used by the Rust side's parity tests
# (`testdata/g2p_battery.tsv`).
JYUTPING_BATTERY = [
    "你好世界", "廣東話", "我愛你", "你好", "開飯", "學校", "音樂",
    "你", "開", "hello", "你good", "𨋢", "乜嘢",
]
KOREAN_BATTERY = [
    "안녕하세요", "감사합니다", "사랑", "한글", "웃음", "가나다", "친구",
    "음악", "가|나", "(웃음)", "학교", "서울",
]


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
    lines = ["# ko_pron tables; columns: rr, rrr (WT-revised), mr (McCune-Reischauer)"]
    for vowel in sorted(ko_vowels):
        row = ko_vowels[vowel]
        lines.append(f"V\t{vowel}\t{row[RR]}\t{row[RRR]}\t{row[MR]}")
    for key in sorted(ko_boundary):
        row = ko_boundary[key]
        lines.append(f"B\t{key}\t{row[RR]}\t{row[RRR]}\t{row[MR]}")
    path = ASSETS / "ko_roman.tsv"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{path.name}: {len(ko_vowels)} vowels, {len(ko_boundary)} boundaries")


def write_jyutping() -> None:
    """Char -> jyutping (sorted-first reading; the Python lib pops a set,
    which is hash-order dependent, so a deterministic pick is used instead)."""
    if not jyutping_dictionary.CHS_DICT or not jyutping_dictionary.CHT_DICT:
        jyutping_dictionary.load_dictionary()
    merged: dict[str, set[str]] = {}
    for source in (jyutping_dictionary.CHS_DICT, jyutping_dictionary.CHT_DICT):
        for character, readings in source.items():
            merged.setdefault(character, set()).update(readings)
    lines = ["# char -> jyutping, sorted-first reading (tones kept)"]
    for character in sorted(merged):
        first = sorted(merged[character])[0]
        lines.append(f"{character}\t{first}")
    path = ASSETS / "jyutping.tsv"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{path.name}: {len(merged)} characters")


def write_g2p_battery() -> None:
    from jyutping import jyutping as jyutping_module
    from ko_pron.ko_pron import romanise

    rows = []
    for word in JYUTPING_BATTERY:
        # All candidate readings per character: the Rust side must pick one
        # of them (Python's own single pick is hash-order dependent).
        expected = [
            sorted(readings) if readings else None
            for readings in jyutping_module.get_jyutping(word, multiple=True)
        ]
        rows.append(
            "jyutping\t"
            + word.replace("\t", " ").replace("\n", " ")
            + "\t"
            + json.dumps(expected, ensure_ascii=False)
        )
    for word in KOREAN_BATTERY:
        for scheme in ("rr", "rrr", "mr"):
            expected = romanise(word, scheme)
            rows.append(f"korean:{scheme}\t{word}\t{expected}")
    path = ROOT / "testdata" / "g2p_battery.tsv"
    path.write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(f"{path.name}: {len(rows)} rows")


def main() -> None:
    ASSETS.mkdir(parents=True, exist_ok=True)
    write_pinyin()
    write_phrases()
    write_kana()
    write_korean()
    write_jyutping()
    write_g2p_battery()


if __name__ == "__main__":
    sys.exit(main())
