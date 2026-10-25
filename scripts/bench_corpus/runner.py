"""Serial benchmark runner: rust / uf3 / py, one process at a time.

Usage:
    python runner.py --tools rust,uf3 --reps 2 --limit 200
    python runner.py --tools py --reps 1

Results append to results.csv (one row per run, flushed immediately), so the
run can be stopped and resumed at any point.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import subprocess
import sys
import time

BASE = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night")
CASES = BASE / "cases.json"
RESULTS = BASE / "results.csv"
WORK = BASE / "work"
RUST = pathlib.Path(r"E:\RUST_LIBRESVIP\RustLibreSVIP\target\release\libresvip-cli.exe")
UF3_CLI = BASE / "uf3_cli.js"
PY = pathlib.Path(r"E:\miniconda\envs\study12\python.exe")
PY_ROOT = pathlib.Path(r"E:\RUST_LIBRESVIP\LibreSVIP")
PY_SCRIPTS = pathlib.Path(r"E:\RUST_LIBRESVIP\RustLibreSVIP\scripts")

FIELDS = [
    "ts", "tool", "tier", "source_ext", "path", "input_bytes", "target",
    "rep", "seconds", "inner_ms", "ok", "out_bytes", "rc", "error",
]

TIMEOUT = {"rust": 600, "uf3": 60, "py": 900}
REPS = {"rust": 2, "uf3": 2, "py": 1}

# Fairness guards.  UtaFormatix3's XML exporters build their output through a
# DOM; under Node that DOM is a polyfill whose insertBefore is linear, so on
# large projects the XML targets measure the shim, not the library.  Those are
# only timed on small files.  Everything above the caps is left to RustLibreSVIP.
UF3_MAX_BYTES = 1_200_000
UF3_XML_TARGETS = {"vsqx", "vsq", "ccs", "s5p", "musicxml", "tssln"}
UF3_XML_MAX_BYTES = 200_000

# UtaFormatix3 Kotlin/JS library, built with
#   gradle -Pkotlin.js.yarn=false :core:browserProductionLibraryDistribution
# and copied next to the npm workspace so its bare imports resolve.
UF3_MODULE = pathlib.Path(
    r"E:\RUST_LIBRESVIP\.refs\utaformatix3\build\js\uf3dist\utaformatix-core.patched.mjs"
)


def load_done() -> set[tuple[str, str, str]]:
    """Runs already attempted, successful or not.

    Failures are kept in results.csv and are not retried on resume: a project
    LibreSVIP rejects (overlapping notes) or that makes UtaFormatix3 time out
    would otherwise be paid again on every restart.
    """
    done: set[tuple[str, str, str]] = set()
    if not RESULTS.exists():
        return done
    with RESULTS.open(encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            done.add((row["path"], row["target"], row["tool"]))
    return done


def run_rust(src: pathlib.Path, out: pathlib.Path) -> tuple[float, int, str, float | None]:
    started = time.perf_counter()
    try:
        proc = subprocess.run(
            [str(RUST), "proj", "convert", str(src), str(out)],
            input="\n" * 128,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=TIMEOUT["rust"],
        )
        return time.perf_counter() - started, proc.returncode, proc.stderr[-400:], None
    except subprocess.TimeoutExpired:
        return time.perf_counter() - started, -9, "timeout", None


def run_uf3(src: pathlib.Path, out: pathlib.Path, ext: str, target: str) -> tuple[float, int, str, float | None]:
    started = time.perf_counter()
    try:
        proc = subprocess.run(
            ["node", str(UF3_CLI), str(src), str(out), ext, target],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=TIMEOUT["uf3"],
        )
        elapsed = time.perf_counter() - started
        inner = None
        for line in proc.stdout.splitlines():
            if line.startswith("{"):
                try:
                    inner = json.loads(line).get("inner_ms")
                except Exception:
                    pass
        return elapsed, proc.returncode, proc.stderr[-400:], inner
    except subprocess.TimeoutExpired:
        return time.perf_counter() - started, -9, "timeout", None


def run_py(src: pathlib.Path, out: pathlib.Path) -> tuple[float, int, str, float | None]:
    state = WORK / "pystate" / (src.stem[:40] + "_" + out.name)
    state.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["PYTHONPATH"] = os.pathsep.join([str(PY_SCRIPTS), str(PY_ROOT)])
    env["PYTHONIOENCODING"] = "utf-8"
    env["COLUMNS"] = "80"
    env["LIBRESVIP_TEST_STATE"] = str(state)
    started = time.perf_counter()
    try:
        proc = subprocess.run(
            [str(PY), "-m", "py_state_entry", "proj", "convert", str(src), str(out)],
            input="\n" * 128,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=TIMEOUT["py"],
            env=env,
            cwd=str(state),
        )
        return time.perf_counter() - started, proc.returncode, proc.stderr[-400:], None
    except subprocess.TimeoutExpired:
        return time.perf_counter() - started, -9, "timeout", None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tools", default="rust,uf3")
    parser.add_argument("--reps", type=int, default=0, help="override repeat count for every tool")
    parser.add_argument("--limit", type=int, default=0, help="max runs this invocation")
    parser.add_argument("--tier", default="", help="only cases from this tier")
    parser.add_argument("--only-ext", default="")
    parser.add_argument("--retry-failed", action="store_true")
    parser.add_argument("--max-input-py", type=int, default=2_000_000,
                        help="skip Python runs for inputs larger than this (bytes)")
    args = parser.parse_args()

    tools = [t.strip() for t in args.tools.split(",") if t.strip()]
    cases = json.loads(CASES.read_text(encoding="utf-8"))
    WORK.mkdir(parents=True, exist_ok=True)
    done = load_done()
    if args.retry_failed:
        done = set()
    new_file = not RESULTS.exists()

    out_handle = RESULTS.open("a", encoding="utf-8", newline="")
    writer = csv.DictWriter(out_handle, fieldnames=FIELDS)
    if new_file:
        writer.writeheader()
        out_handle.flush()

    runs = 0
    skipped: list[tuple[str, str, int]] = []
    poisoned: set[str] = set()
    if RESULTS.exists():
        with RESULTS.open(encoding="utf-8", newline="") as handle:
            for row in csv.DictReader(handle):
                if row.get("rc") == "-9":
                    poisoned.add(row["path"])
    started_all = time.perf_counter()
    for case in cases:
        if args.tier and case.get("tier") != args.tier:
            continue
        ext = case["ext"].lstrip(".")
        if args.only_ext and ext != args.only_ext:
            continue
        src = pathlib.Path(case["path"])
        if not src.exists():
            continue
        folder = WORK / f'{case["tier"]}_{src.stem[:40]}_{case["size"]}'
        folder.mkdir(parents=True, exist_ok=True)
        for target in case["targets"]:
            for tool in tools:
                if tool not in case["tools"]:
                    continue
                if tool == "py" and case["size"] > args.max_input_py:
                    continue
                if tool == "uf3" and case["size"] > UF3_MAX_BYTES:
                    skipped.append((tool, target, case["size"]))
                    continue
                if tool == "uf3" and target in UF3_XML_TARGETS and case["size"] > UF3_XML_MAX_BYTES:
                    skipped.append((tool, target, case["size"]))
                    continue
                if tool == "uf3" and case["path"] in poisoned:
                    skipped.append((tool, target, case["size"]))
                    continue
                if (case["path"], target, tool) in done:
                    continue
                reps = args.reps or REPS[tool]
                for rep in range(1, reps + 1):
                    out = folder / f'{tool}_{target}.{target}'
                    if out.exists():
                        out.unlink()
                    if tool == "rust":
                        secs, rc, err, inner = run_rust(src, out)
                    elif tool == "uf3":
                        secs, rc, err, inner = run_uf3(src, out, ext, target)
                    else:
                        secs, rc, err, inner = run_py(src, out)
                    ok = rc == 0 and out.exists() and out.stat().st_size > 0
                    if not ok and rc == -9:
                        # UtaFormatix3 (Kotlin/JS) can thrash for many minutes on
                        # one dense project; skip its remaining targets instead
                        # of paying the timeout once per format.
                        poisoned.add(case["path"])
                    writer.writerow({
                        "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
                        "tool": tool,
                        "tier": case["tier"],
                        "source_ext": ext,
                        "path": case["path"],
                        "input_bytes": case["size"],
                        "target": target,
                        "rep": rep,
                        "seconds": f"{secs:.4f}",
                        "inner_ms": "" if inner is None else f"{inner:.1f}",
                        "ok": int(bool(ok)),
                        "out_bytes": out.stat().st_size if out.exists() else 0,
                        "rc": rc,
                        "error": (err or "").replace("\n", " ")[:300],
                    })
                    out_handle.flush()
                    runs += 1
                    status = "ok" if ok else f"FAIL rc={rc}"
                    print(f"[{runs}] {tool:>4} {ext:>8} -> {target:<8} {secs:7.2f}s {status}", flush=True)
                    if args.limit and runs >= args.limit:
                        out_handle.close()
                        print(f"limit reached ({runs} runs, {time.perf_counter()-started_all:.0f}s)")
                        return 0
    out_handle.close()
    if skipped:
        print(f"skipped {len(skipped)} uf3 runs by size guard (max {UF3_MAX_BYTES} bytes, xml targets {UF3_XML_MAX_BYTES})")
    print(f"done: {runs} runs in {time.perf_counter()-started_all:.0f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
