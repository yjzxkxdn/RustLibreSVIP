"""Time the native Rust build per conversion, split into read and write.

Runs `phases.exe <input> <output>` once per (source file, target format) pair --
one cold process per conversion, like every other measurement in this
benchmark -- and records the reader (`load_ms`) and writer (`dump_ms`) halves
separately.

Resumable and filterable:

    python native_phases.py [--retry-failed] [--only core|heavy] [--limit N]
"""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys
import time

BASE = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night")
CASES = BASE / "cases.json"
PHASES = pathlib.Path(
    r"E:\RUST_LIBRESVIP\RustLibreSVIP\target\release\examples\phases.exe"
)
OUT_CSV = BASE / "phases_native.csv"
WORK = BASE / "native"
HEADER = [
    "tier", "source_ext", "target", "path", "input_bytes", "out_bytes",
    "load_ms", "dump_ms", "total_ms", "ok", "error",
]


def build_jobs() -> list[dict]:
    cases = json.loads(CASES.read_text(encoding="utf-8"))
    jobs: list[dict] = []
    for case in cases:
        if case["tier"] == "stress":
            continue
        if not pathlib.Path(case["path"]).exists():
            continue
        for target in case["targets"]:
            jobs.append({
                "tier": case["tier"],
                "path": case["path"],
                "ext": case["ext"].lstrip("."),
                "size": case["size"],
                "target": target,
            })
    return jobs


def load_done(retry_failed: bool) -> set[tuple[str, str]]:
    done: set[tuple[str, str]] = set()
    if not OUT_CSV.exists():
        return done
    with OUT_CSV.open(encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            if retry_failed or row.get("ok") == "1":
                done.add((row["path"], row["target"]))
    return done


def main() -> int:
    retry_failed = "--retry-failed" in sys.argv
    only = None
    if "--only" in sys.argv:
        only = sys.argv[sys.argv.index("--only") + 1]
    limit = None
    if "--limit" in sys.argv:
        limit = int(sys.argv[sys.argv.index("--limit") + 1])
    jobs = [job for job in build_jobs() if only is None or job["tier"] == only]
    done = load_done(retry_failed)
    todo = [job for job in jobs if (job["path"], job["target"]) not in done]
    if limit is not None:
        todo = todo[:limit]
    print(f"{len(jobs)} jobs, {len(done)} done, {len(todo)} to run", flush=True)
    if not todo:
        return 0

    new_file = not OUT_CSV.exists()
    handle = OUT_CSV.open("a", encoding="utf-8", newline="")
    writer = csv.writer(handle)
    if new_file:
        writer.writerow(HEADER)
    started_all = time.perf_counter()
    failures = 0
    for index, job in enumerate(todo, 1):
        source = pathlib.Path(job["path"])
        folder = WORK / f'{job["ext"]}_{job["target"]}_{job["size"]}'
        folder.mkdir(parents=True, exist_ok=True)
        output = folder / f"{source.stem[:48]}.{job['target']}"
        # Warm the OS cache first: some of the corpus lives on a cloud-sync
        # virtual drive, and without this the first target of a case pays the
        # whole drive latency inside `load_ms` (one 493 KB file showed 235 ms
        # cold and 14 ms warm). The wasm harness reads the file outside the
        # timed section, so this keeps the two sides comparable.
        try:
            source.read_bytes()
        except OSError:
            pass
        started = time.perf_counter()
        try:
            proc = subprocess.run(
                [str(PHASES), str(source), str(output)],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=300,
            )
            total_ms = (time.perf_counter() - started) * 1000.0
            payload = {}
            for line in (proc.stdout or "").splitlines():
                if line.startswith("{"):
                    payload = json.loads(line)
            error = "" if payload else ((proc.stderr or "").strip().splitlines() or ["no output"])[0][:200]
            ok = int(bool(payload))
        except subprocess.TimeoutExpired:
            total_ms = (time.perf_counter() - started) * 1000.0
            payload = {}
            ok = 0
            error = "timeout after 300 s"
        writer.writerow([
            job["tier"], job["ext"], job["target"], job["path"], job["size"],
            payload.get("out_bytes", 0),
            f'{payload.get("load_ms", 0):.3f}',
            f'{payload.get("dump_ms", 0):.3f}',
            f"{total_ms:.1f}",
            ok, error,
        ])
        handle.flush()
        if not ok:
            failures += 1
        if index % 50 == 0:
            print(f"{index}/{len(todo)}  {time.perf_counter() - started_all:.0f}s  failed={failures}", flush=True)
    handle.close()
    print(
        f"wrote {OUT_CSV} ({len(todo)} rows, {failures} failed) "
        f"in {time.perf_counter() - started_all:.0f}s",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
