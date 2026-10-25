"""Re-measure note agreement with a one-tick tolerance.

Several engines disagree by exactly one tick on note boundaries when the source
stores milliseconds (SVP) and the tick grid is derived.  A strict tuple compare
then reports 0% identical notes even though every note is present with the same
key and lyric, so the same UFData pairs are re-compared with +/-1 and +/-2 tick
tolerance.
"""

from __future__ import annotations

import csv
import pathlib
import sys

sys.path.insert(0, r"E:\RUST_LIBRESVIP\.tmp\bench-night")
import consistency as C  # noqa: E402

BASE = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night")


def match(a: list[tuple], b: list[tuple], tolerance: int) -> int:
    """Greedy in-order match allowing |da|,|db| <= tolerance."""
    matched = 0
    index = 0
    for note in a:
        while index < len(b):
            other = b[index]
            if (
                abs(note[0] - other[0]) <= tolerance
                and abs(note[1] - other[1]) <= tolerance
                and note[2] == other[2]
                and note[3] == other[3]
            ):
                matched += 1
                index += 1
                break
            index += 1
    return matched


def main() -> int:
    rows = list(csv.DictReader((BASE / "consistency_cross.csv").open(encoding="utf-8")))
    per_pair: dict[str, list[tuple[float, float, float]]] = {}
    for row in rows:
        src = pathlib.Path(row["path"])
        case = None
        for folder in (BASE / "work").iterdir():
            if folder.name.endswith(str(row["input_bytes"])) and src.stem[:40] in folder.name:
                case = folder
                break
        if case is None:
            continue
        tag = "_py" if "py" in row["pair"] else ""
        left = case / f"cons_rust{tag}.ufdata"
        right_tool = "uf3" if "uf3" in row["pair"] else "py"
        right = case / f"cons_{right_tool}{tag}.ufdata"
        if not left.exists() or not right.exists():
            continue
        a = C.load_ufdata(left)
        b = C.load_ufdata(right)
        if a is None or b is None:
            continue
        notes_a = [n for t in a["tracks"] for n in t["notes"]]
        notes_b = [n for t in b["tracks"] for n in t["notes"]]
        if not notes_a or not notes_b:
            continue
        base = max(len(notes_a), len(notes_b))
        strict = sum(1 for x, y in zip(notes_a, notes_b) if x == y)
        one = match(notes_a, notes_b, 1)
        two = match(notes_a, notes_b, 2)
        per_pair.setdefault(row["pair"], []).append(
            (100 * strict / base, 100 * one / base, 100 * two / base)
        )

    for pair, values in sorted(per_pair.items()):
        n = len(values)
        strict = sum(v[0] for v in values) / n
        one = sum(v[1] for v in values) / n
        two = sum(v[2] for v in values) / n
        print(f"{pair}: n={n} strict={strict:.1f}% +/-1 tick={one:.1f}% +/-2 ticks={two:.1f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
