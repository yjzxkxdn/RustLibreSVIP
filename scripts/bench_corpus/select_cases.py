"""Turn the filesystem scan into a benchmark case list.

Tiers:
  core  - ~1 project per (extension, size band), source readable by all three
          tools; targets limited to the four every tool can write.
  extra - LibreSVIP-only formats (acep/svip/mid/...): rust + py, no uf3.
  py    - the subset of `core` also run through the Python reference.
"""

from __future__ import annotations

import json
import pathlib
import random
import re

SCAN = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night\scan.jsonl")
OUT = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night\cases.json")

# Formats UtaFormatix3 can both read and that LibreSVIP also reads.
UF3_SOURCES = {"ustx", "svp", "vsqx", "vsq", "ust", "ccs", "s5p", "musicxml", "mxl", "dv", "vpr", "ppsf", "ufdata", "tssln"}
CROSS_TARGETS = ["ufdata", "svp", "vsqx", "ustx"]

# A project never converts to its own format.
def targets_for(ext: str, allowed: list[str]) -> list[str]:
    own = {"mxl": "musicxml"}.get(ext, ext)
    return [t for t in allowed if t != own]


def main() -> int:
    rows = [json.loads(line) for line in SCAN.read_text(encoding="utf-8", errors="replace").splitlines() if line.strip()]
    # de-duplicate mirror copies (same basename + size), prefer the shorter path
    best: dict[tuple[str, int], dict] = {}
    for row in rows:
        key = (pathlib.Path(row["path"]).name.lower(), row["size"])
        current = best.get(key)
        if current is None or len(row["path"]) < len(current["path"]):
            best[key] = row
    unique = list(best.values())

    # drop autosave copies when a normal sibling exists
    names = {pathlib.Path(r["path"]).name.lower() for r in unique}
    filtered = []
    for row in unique:
        name = pathlib.Path(row["path"]).name.lower()
        if "-autosave" in name:
            sibling = name.replace("-autosave", "")
            if sibling in names:
                continue
        if "_autosave" in name or "autosave" in name and any(
            n == name.replace("autosave", "").replace("--", "-") for n in names
        ):
            continue
        filtered.append(row)

    def band(size: int) -> str:
        if size < 100 * 1024:
            return "tiny"
        if size < 500 * 1024:
            return "small"
        if size < 2 * 1024 * 1024:
            return "medium"
        if size < 8 * 1024 * 1024:
            return "large"
        return "huge"

    rng = random.Random(20261025)
    core: list[dict] = []
    extra: list[dict] = []

    for ext, limit in [
        ("ustx", 34), ("svp", 30), ("vsqx", 2), ("vsq", 1), ("ust", 2),
        ("ufdata", 2), ("vpr", 1), ("s5p", 1), ("ccs", 1), ("musicxml", 1),
        ("mxl", 1),
    ]:
        pool = [r for r in filtered if r["ext"] == "." + ext and r["size"] <= 8 * 1024 * 1024]
        by_band: dict[str, list[dict]] = {}
        for row in pool:
            by_band.setdefault(band(row["size"]), []).append(row)
        picked: list[dict] = []
        # round-robin over bands so every size class shows up
        bands = ["tiny", "small", "medium", "large"]
        while len(picked) < limit and any(by_band.get(b) for b in bands):
            for b in bands:
                if len(picked) >= limit:
                    break
                bucket = by_band.get(b) or []
                if bucket:
                    picked.append(bucket.pop(rng.randrange(len(bucket))))
        for row in picked:
            core.append({**row, "tier": "core"})

    for ext, limit in [
        ("acep", 10), ("svip", 5), ("svip3", 2), ("mid", 14), ("dspx", 1), ("svip3", 1),
    ]:
        pool = [r for r in filtered if r["ext"] == "." + ext and r["size"] <= 4 * 1024 * 1024]
        rng.shuffle(pool)
        for row in pool[:limit]:
            extra.append({**row, "tier": "extra"})

    # py sample: 2 per format, spread over size bands
    py_pick: list[dict] = []
    for ext in ("ustx", "svp", "mid", "acep"):
        pool = [r for r in core + extra if r["ext"] == "." + ext]
        if not pool:
            continue
        pool = sorted(pool, key=lambda r: r["size"])
        if len(pool) >= 2:
            py_pick.extend([pool[0], pool[len(pool) // 2]])
        else:
            py_pick.extend(pool)

    cases = []
    for row in core:
        ext = row["ext"].lstrip(".")
        targets = targets_for(ext, CROSS_TARGETS)
        cases.append({**row, "targets": targets, "tools": ["rust", "uf3"]})
    for row in extra:
        ext = row["ext"].lstrip(".")
        cases.append({**row, "targets": targets_for(ext, CROSS_TARGETS), "tools": ["rust"]})
    for row in py_pick:
        for case in cases:
            if case["path"] == row["path"]:
                case["tools"] = sorted(set(case["tools"]) | {"py"})
                break

    # stress cases: the largest files, rust only, one target each
    stress = []
    for ext in ("svp", "svip3", "ustx"):
        pool = sorted((r for r in filtered if r["ext"] == "." + ext), key=lambda r: -r["size"])
        for row in pool[:2]:
            extn = ext
            stress.append({**row, "targets": targets_for(extn, CROSS_TARGETS)[:2], "tools": ["rust"], "tier": "stress"})

    all_cases = cases + stress
    OUT.write_text(json.dumps(all_cases, indent=2, ensure_ascii=False), encoding="utf-8")
    counts: dict[str, int] = {}
    for row in all_cases:
        counts[row["tier"]] = counts.get(row["tier"], 0) + 1
    print("cases:", len(all_cases), counts)
    runs = sum(len(c["targets"]) * len(c["tools"]) for c in all_cases)
    print("planned runs (1 rep):", runs)
    for row in all_cases[:8]:
        print(f'  {row["tier"]:>6} {row["ext"]:>7} {int(row["size"]/1024):>7} KB {row["tools"]} -> {row["targets"]}')
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
