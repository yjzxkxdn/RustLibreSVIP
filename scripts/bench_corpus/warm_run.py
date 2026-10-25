"""Measure the warm path: many conversions inside one long-lived process.

Cold numbers (one process per conversion) come from `phases_native.csv`; this
script re-converts one representative file per (source, target) pair several
times inside a single native process and a single WebAssembly instance, and
writes `warm_cold.csv` with both.

    python warm_run.py [--rounds 4] [--max-bytes 1500000]

Round 1 is still cold-ish (the process has started but its caches are empty);
rounds 2+ are the steady state that a GUI or an editor session lives in.
"""

from __future__ import annotations

import csv
import json
import os
import pathlib
import statistics
import subprocess
import sys

BASE = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night")
REPO = pathlib.Path(r"E:\RUST_LIBRESVIP\RustLibreSVIP")
PHASES = REPO / "target" / "release" / "examples" / "phases.exe"
WASM = REPO / "target" / "wasm32-unknown-unknown" / "wasm" / "libresvip_web.wasm"
BATCH = REPO / "scripts" / "bench_corpus" / "wasm_batch2.js"
FORMATS = BASE / "wasm_formats.json"
JOBS = BASE / "warm_jobs.tsv"
OUT_CSV = BASE / "warm_cold.csv"
WORK = BASE / "warm"
FIELDS = [
    "round", "source_ext", "target", "path", "input_bytes",
    "native_load_ms", "native_dump_ms", "native_total_ms",
    "wasm_convert_ms", "wasm_ok", "wasm_memory_mb",
]


def representative_files(cold: list[dict], max_bytes: int) -> list[dict]:
    """One file per pair: the one closest to that pair's median input size."""
    by_pair: dict[tuple[str, str], list[dict]] = {}
    for row in cold:
        if row["ok"] != "1":
            continue
        size = int(row["input_bytes"])
        if size > max_bytes:
            continue
        by_pair.setdefault((row["source_ext"], row["target"]), []).append(row)
    picked = []
    for key in sorted(by_pair):
        rows = sorted(by_pair[key], key=lambda row: int(row["input_bytes"]))
        picked.append(rows[len(rows) // 2])
    return picked


def suffix_ids() -> dict[str, str]:
    formats = json.loads(FORMATS.read_text(encoding="utf-8"))
    mapping: dict[str, str] = {}
    for fmt in formats:
        for suffix in fmt.get("suffixes") or []:
            mapping.setdefault(suffix.lower(), fmt["id"])
    return mapping


def main() -> int:
    rounds = 4
    if "--rounds" in sys.argv:
        rounds = int(sys.argv[sys.argv.index("--rounds") + 1])
    max_bytes = 1_500_000
    if "--max-bytes" in sys.argv:
        max_bytes = int(sys.argv[sys.argv.index("--max-bytes") + 1])

    cold = list(csv.DictReader((BASE / "phases_native.csv").open(encoding="utf-8")))
    picked = representative_files(cold, max_bytes)
    ids = suffix_ids()
    WORK.mkdir(parents=True, exist_ok=True)

    pairs: list[tuple[str, str, str, str, str]] = []
    for row in picked:
        source, target = row["source_ext"], row["target"]
        source_path = pathlib.Path(row["path"])
        folder = WORK / f"{source}_{target}"
        folder.mkdir(parents=True, exist_ok=True)
        output = folder / f"{source_path.stem[:40]}.{target}"
        pairs.append((row["path"], str(output), source, target, row["input_bytes"]))

    # the job list is the pair list repeated `rounds` times: the process (and
    # the wasm instance) sees round 2 while the round-1 caches are already warm
    lines = []
    for _ in range(rounds):
        for path_in, path_out, source, target, _ in pairs:
            lines.append("\t".join((path_in, path_out, ids.get(source, ""), ids.get(target, ""))))
    JOBS.write_text("\n".join(lines) + "\n", encoding="utf-8")
    total_jobs = len(lines)
    print(f"{len(pairs)} pairs x {rounds} rounds = {total_jobs} conversions", flush=True)

    # native: one process for every job
    proc = subprocess.run(
        [str(PHASES), "--jobs", str(JOBS)], capture_output=True, text=True,
        encoding="utf-8", errors="replace", timeout=3600,
    )
    native_payloads: dict[int, dict] = {}
    for line in (proc.stdout or "").splitlines():
        if line.startswith("{"):
            data = json.loads(line)
            if "i" in data:
                native_payloads[data["i"]] = data
    print(f"native done: rc={proc.returncode}, {len(native_payloads)}/{total_jobs} jobs", flush=True)

    # wasm: one module instance for every job
    env = os.environ.copy()
    env["WASM_MODULE"] = str(WASM)
    wasm_proc = subprocess.run(
        ["node", str(BATCH), str(JOBS), str(WORK)], capture_output=True, text=True,
        encoding="utf-8", errors="replace", env=env, timeout=3600,
    )
    wasm_payloads: dict[int, dict] = {}
    for line in (wasm_proc.stdout or "").splitlines():
        if line.startswith("{"):
            data = json.loads(line)
            if "i" in data:
                wasm_payloads[data["i"]] = data
            else:
                print("wasm summary:", line[:200], flush=True)
    print(f"wasm done: rc={wasm_proc.returncode}, {len(wasm_payloads)}/{total_jobs} jobs", flush=True)
    if wasm_proc.returncode != 0:
        print((wasm_proc.stderr or "")[:300], flush=True)

    rows = []
    for round_index in range(1, rounds + 1):
        offset = (round_index - 1) * len(pairs)
        for index, (path_in, _, source, target, size) in enumerate(pairs):
            native = native_payloads.get(offset + index, {})
            wasm = wasm_payloads.get(offset + index, {})
            rows.append({
                "round": round_index,
                "source_ext": source,
                "target": target,
                "path": path_in,
                "input_bytes": size,
                "native_load_ms": round(native.get("load_ms", 0.0), 3),
                "native_dump_ms": round(native.get("dump_ms", 0.0), 3),
                "native_total_ms": round(native.get("load_ms", 0.0) + native.get("dump_ms", 0.0), 3),
                "wasm_convert_ms": round(wasm.get("convert_ms", 0.0), 3),
                "wasm_ok": int(bool(wasm.get("ok"))),
                "wasm_memory_mb": round(wasm.get("memory_mb", 0.0), 1),
            })

    with OUT_CSV.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=FIELDS)
        writer.writeheader()
        writer.writerows(rows)

    cold_by_key = {
        (row["path"].lower(), row["target"]): float(row["total_ms"])
        for row in cold if row["ok"] == "1"
    }
    print()
    print("| source → target | file | cold ms | warm r1 ms | warm r2+ ms | cold/warm | wasm warm ms |")
    print("|---|---|---|---|---|---|---|")
    native_all, wasm_all = [], []
    for index, (path_in, _, source, target, _) in enumerate(pairs):
        mine = [row for row in rows if row["source_ext"] == source and row["target"] == target]
        first = mine[0]["native_total_ms"]
        later = statistics.median(row["native_total_ms"] for row in mine[1:])
        cold_ms = cold_by_key.get((path_in.lower(), target))
        wasm_warm = statistics.median(row["wasm_convert_ms"] for row in mine[1:])
        native_all.append(later)
        wasm_all.append(wasm_warm)
        ratio = f"{cold_ms / later:.1f}x" if cold_ms and later else "—"
        print(f"| `{source}` → `{target}` | {pathlib.Path(path_in).name[:26]} | "
              f"{cold_ms:.0f} | {first:.0f} | {later:.0f} | {ratio} | {wasm_warm:.0f} |")
    print()
    print(f"native warm median {statistics.median(native_all):.1f} ms, "
          f"cold median {statistics.median([cold_by_key[(p[0].lower(), p[3])] for p in pairs if (p[0].lower(), p[3]) in cold_by_key]):.1f} ms")
    print(f"wasm warm median {statistics.median(wasm_all):.1f} ms")
    print(f"wrote {OUT_CSV}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
