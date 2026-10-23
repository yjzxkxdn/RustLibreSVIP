"""Benchmark the Rust CLI against the Python reference.

For each conversion pair the script runs both implementations on the same input
(with every prompt answered by its default), times them, and compares output
bytes.

    E:\\miniconda\\envs\\study12\\python.exe scripts/bench.py
    ... scripts/bench.py --pairs json:svp,json:ust --repeat 5
    ... scripts/bench.py --input testdata/bench_large.json
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import pathlib
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
PY_ROOT = ROOT / "LibreSVIP"
RUST_EXE = ROOT / "RustLibreSVIP" / "target" / "release" / "libresvip-cli.exe"
TESTDATA = pathlib.Path(__file__).resolve().parent.parent / "testdata"
PYTHON = pathlib.Path(r"E:\miniconda\envs\study12\python.exe")

STDIN = "\n" * 64


def python_env() -> dict[str, str]:
    env = os.environ.copy()
    env["PYTHONPATH"] = str(PY_ROOT)
    env["PYTHONIOENCODING"] = "utf-8"
    env["COLUMNS"] = "80"
    return env


def run_once(command: list[str], workdir: pathlib.Path, env: dict[str, str]) -> float:
    start = time.perf_counter()
    subprocess.run(
        command,
        input=STDIN,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
        cwd=str(workdir),
        check=False,
    )
    return time.perf_counter() - start


UUID_RE = __import__("re").compile(
    r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
)


def file_digest(path: pathlib.Path) -> str:
    if not path.exists():
        return "-"
    data = path.read_bytes()
    try:
        data = UUID_RE.sub("<uuid>", data.decode("utf-8")).encode("utf-8")
    except UnicodeDecodeError:
        pass
    return hashlib.sha256(data).hexdigest()


def acep_project(path: pathlib.Path) -> dict | None:
    """Decode the project payload of an `.acep` file, or None if unreadable."""
    try:
        import zstandard
    except ImportError:
        return None
    try:
        outer = json.loads(path.read_text("utf-8"))
        raw = zstandard.ZstdDecompressor().decompress(base64.b64decode(outer["content"]))
        return json.loads(raw)
    except (ValueError, KeyError, zstandard.ZstdError):
        return None


def strip_random_colors(value):
    """ACE Studio picks a random track colour, in Python as well as in Rust."""
    if isinstance(value, dict):
        return {
            key: (
                "<random>"
                if key in {"color", "colorIndex", "textColor", "headerColor"}
                else strip_random_colors(item)
            )
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [strip_random_colors(item) for item in value]
    return value


def acep_digest(path: pathlib.Path) -> str | None:
    project = acep_project(path)
    if project is None:
        return None
    text = json.dumps(strip_random_colors(project), sort_keys=True, ensure_ascii=False)
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pairs", default="json:json,json:svp,json:ust,json:vsqx,json:mid")
    parser.add_argument("--input", default="bench_large.json")
    parser.add_argument("--repeat", type=int, default=3)
    args = parser.parse_args()

    if not RUST_EXE.exists():
        print(f"missing release binary: {RUST_EXE}\nrun: cargo build --release")
        return 2

    source = TESTDATA / args.input
    if not source.exists():
        print(f"missing input {source}\nrun: scripts/gen_bench.py")
        return 2

    pairs = [tuple(pair.split(":")) for pair in args.pairs.split(",")]
    print(f"input: {source.name} ({source.stat().st_size / 1024:.1f} KiB), repeat={args.repeat}\n")
    header = f"{'pair':<14}{'python (s)':>12}{'rust (s)':>11}{'speedup':>9}{'bytes':>10}  output"
    print(header)
    print("-" * len(header))

    for source_ext, target_ext in pairs:
        with tempfile.TemporaryDirectory() as temp:
            work = pathlib.Path(temp)
            for tag in ("py", "rs"):
                (work / tag).mkdir()
                (work / tag / f"input.{source_ext}").write_bytes(source.read_bytes())

            env_py = python_env()
            env_rs = os.environ.copy()
            env_rs["COLUMNS"] = "80"

            py_command = [
                str(PYTHON),
                "-m",
                "libresvip.cli",
                "proj",
                "convert",
                f"input.{source_ext}",
                f"out.{target_ext}",
            ]
            rs_command = [
                str(RUST_EXE),
                "proj",
                "convert",
                f"input.{source_ext}",
                f"out.{target_ext}",
            ]

            py_times = []
            rs_times = []
            for _ in range(args.repeat):
                py_times.append(run_once(py_command, work / "py", env_py))
                rs_times.append(run_once(rs_command, work / "rs", env_rs))

            py_out = work / "py" / f"out.{target_ext}"
            rs_out = work / "rs" / f"out.{target_ext}"
            same = file_digest(py_out) == file_digest(rs_out) and py_out.exists()
            raw_same = (
                py_out.exists()
                and rs_out.exists()
                and py_out.read_bytes() == rs_out.read_bytes()
            )
            py_time = statistics.median(py_times)
            rs_time = statistics.median(rs_times)
            speedup = py_time / rs_time if rs_time else float("inf")
            size = py_out.stat().st_size if py_out.exists() else 0
            marker = "identical" if raw_same else ("identical(uuid)" if same else "DIFFERENT")
            if not same and target_ext == "acep":
                py_digest = acep_digest(py_out)
                if py_digest is not None and py_digest == acep_digest(rs_out):
                    marker = "identical(colour)"
            print(
                f"{source_ext}->{target_ext:<9}{py_time:>12.3f}{rs_time:>11.3f}"
                f"{speedup:>8.2f}x{size:>10}  {marker}"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())
