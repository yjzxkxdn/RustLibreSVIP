"""Run the WebAssembly build on every benchmark case, one cold process per conversion.

Each conversion gets a fresh Node process (and therefore a fresh module
instance), which is what the browser pays per page load; `instantiate_ms` is
that one-off cost and `convert_ms` the conversion itself. A job that traps
(wasm `unreachable` = allocation failure) only kills its own process, so the
result file always covers every scheduled pair.

Resumable: rows already present in the CSV are skipped, and `--retry-failed`
re-runs the ones that failed before.

    python wasm_run2.py [--retry-failed] [--limit N]
"""

from __future__ import annotations

import csv
import json
import os
import pathlib
import subprocess
import sys
import time

BASE = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night")
CASES = BASE / "cases.json"
FORMATS = BASE / "wasm_formats.json"
WASM = pathlib.Path(
    r"E:\RUST_LIBRESVIP\RustLibreSVIP\target\wasm32-unknown-unknown\wasm\libresvip_web.wasm"
)
CLI = BASE / "wasm_cli.js"
OUT_CSV = BASE / "results_wasm2.csv"
WORK = BASE / "wasm2"
HEADER = [
    "source_ext", "target", "path", "input_bytes", "out_bytes",
    "instantiate_ms", "convert_ms", "ok", "error",
]


def build_jobs() -> list[dict]:
    cases = json.loads(CASES.read_text(encoding="utf-8"))
    formats = json.loads(FORMATS.read_text(encoding="utf-8"))
    suffix_to_id: dict[str, str] = {}
    for fmt in formats:
        for suffix in fmt.get("suffixes") or []:
            suffix_to_id.setdefault(suffix.lower(), fmt["id"])
    jobs: list[dict] = []
    for case in cases:
        if case["tier"] == "stress":
            continue
        if not pathlib.Path(case["path"]).exists() or case["size"] > 4_000_000:
            continue
        src_id = suffix_to_id.get(case["ext"].lstrip(".").lower())
        if src_id is None:
            continue
        for target in case["targets"]:
            dst_id = suffix_to_id.get(target)
            if dst_id is None:
                continue
            jobs.append({
                "path": case["path"],
                "ext": case["ext"].lstrip("."),
                "size": case["size"],
                "target": target,
                "src": src_id,
                "dst": dst_id,
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
    limit = None
    if "--limit" in sys.argv:
        limit = int(sys.argv[sys.argv.index("--limit") + 1])
    jobs = build_jobs()
    done = load_done(retry_failed)
    todo = [job for job in jobs if (job["path"], job["target"]) not in done]
    if limit is not None:
        todo = todo[:limit]
    print(f"{len(jobs)} jobs, {len(done)} already done, {len(todo)} to run", flush=True)
    if not todo:
        return 0

    env = os.environ.copy()
    env["WASM_MODULE"] = str(WASM)
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
        started = time.perf_counter()
        try:
            proc = subprocess.run(
                ["node", str(CLI), str(source), str(output), job["src"], job["dst"]],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                env=env, timeout=180,
            )
            payload = {}
            for line in (proc.stdout or "").splitlines():
                if line.startswith("{"):
                    payload = json.loads(line)
            error = ""
            if not payload:
                error = ((proc.stderr or "").strip().splitlines() or ["no output"])[0][:200]
            ok = int(bool(payload.get("ok")))
            if not ok and not error:
                error = ((proc.stderr or "").strip().splitlines() or ["failed"])[0][:200]
        except subprocess.TimeoutExpired:
            payload = {}
            ok = 0
            error = "timeout after 180 s"
        writer.writerow([
            job["ext"], job["target"], job["path"], job["size"],
            payload.get("bytes", 0),
            f'{payload.get("load_ms", 0):.2f}',
            f'{payload.get("convert_ms", 0):.2f}',
            ok, error,
        ])
        handle.flush()
        if not ok:
            failures += 1
        if index % 25 == 0:
            elapsed = time.perf_counter() - started_all
            print(f"{index}/{len(todo)}  {elapsed:.0f}s  failures={failures}", flush=True)
    handle.close()
    print(
        f"wrote {OUT_CSV} ({len(todo)} rows, {failures} failed) "
        f"in {time.perf_counter() - started_all:.0f}s",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
