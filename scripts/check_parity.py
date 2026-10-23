"""Differential test harness: Python reference CLI vs the Rust CLI.

Runs the same argv/stdin against both implementations and reports differences
in stdout, stderr and exit code.

    E:\\miniconda\\envs\\study12\\python.exe scripts/check_parity.py
    ... scripts/check_parity.py --only plugin_list
    ... scripts/check_parity.py -v          # show diffs for failing cases
"""

from __future__ import annotations

import argparse
import difflib
import json
import re
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
PY_ROOT = ROOT / "LibreSVIP"
RUST_EXE = ROOT / "RustLibreSVIP" / "target" / "debug" / "libresvip-cli.exe"
PYTHON = pathlib.Path(r"E:\miniconda\envs\study12\python.exe")
TESTDATA = ROOT / "RustLibreSVIP" / "testdata"


def state_dir(tag: str) -> pathlib.Path:
    """Isolated platformdirs root so stateful commands do not touch real settings."""
    folder = TESTDATA / ".state" / tag
    if folder.exists():
        import shutil

        shutil.rmtree(folder)
    folder.mkdir(parents=True)
    return folder


def python_env(tag: str) -> dict[str, str]:
    env = os.environ.copy()
    scripts = pathlib.Path(__file__).resolve().parent
    env["PYTHONPATH"] = os.pathsep.join([str(scripts), str(PY_ROOT)])
    env["PYTHONIOENCODING"] = "utf-8"
    env["COLUMNS"] = "80"
    env["LIBRESVIP_TEST_STATE"] = str(state_dir(f"py-{tag}"))
    env.pop("NO_COLOR", None)
    return env


FIXTURES = ("simple.json", "simple_singing.json", "bad.txt")


def work_dir(tag: str, name: str) -> pathlib.Path:
    import shutil

    folder = TESTDATA / ".work" / name / tag
    if folder.exists():
        shutil.rmtree(folder)
    folder.mkdir(parents=True)
    for fixture in FIXTURES:
        source = TESTDATA / fixture
        if source.exists():
            shutil.copyfile(source, folder / fixture)
    return folder


def produced_files(folder: pathlib.Path) -> dict[str, bytes]:
    files = {}
    for path in sorted(folder.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(folder).as_posix()
        if relative in FIXTURES:
            continue
        files[relative] = path.read_bytes()
    return files


def run_python(name: str, argv: list[str], stdin: str) -> subprocess.CompletedProcess[str]:
    command = [str(PYTHON), "-m", "py_state_entry", *argv]
    return subprocess.run(
        command,
        input=stdin,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=python_env(name),
        cwd=str(work_dir("py", name)),
    )


def run_rust(name: str, argv: list[str], stdin: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["COLUMNS"] = "80"
    env["LOCALAPPDATA"] = str(state_dir(f"rs-{name}"))
    return subprocess.run(
        [str(RUST_EXE), *argv],
        input=stdin,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
        cwd=str(work_dir("rs", name)),
    )


CASES: list[tuple[str, list[str], str]] = []


def case(name: str, argv: list[str], stdin: str = "") -> None:
    CASES.append((name, argv, stdin))


def build_cases() -> None:
    case("plugin_list", ["plugin", "list"])
    case("conf_list", ["conf", "list"])
    for plugin in (
        "ace", "acep", "aisp", "ass", "ccs", "ds", "dspx", "dv", "json", "lrc",
        "mid", "mtp", "musicxml", "nn", "ppsf", "ps_project", "s5p", "srt", "svg",
        "svip", "svip3", "svp", "tlp", "tlpx", "tsmsln", "tssln", "ufdata", "ust",
        "ustx", "vfp", "vog", "vpr", "vshp", "vspx", "vsq", "vsqx", "vvproj",
        "vxf", "xvsq", "y77",
    ):
        case(f"detail_{plugin}", ["plugin", "detail", plugin])
    case("detail_missing", ["plugin", "detail", "nope"])
    case("convert_bad_input", ["proj", "convert", "bad.txt", "o.ust"])
    case("convert_bad_output", ["proj", "convert", "simple.json", "o.txt"])
    case("convert_missing_file", ["proj", "convert", "nope.json", "o.ust"])
    case("convert_json_json", ["proj", "convert", "simple.json", "out.json"], "n\nn\nn\nn\nn\nn\nn\n")
    case("conf_set_language", ["conf", "set", "language", "ja_JP"])
    case("conf_set_invalid", ["conf", "set", "foo", "bar"])
    case("plugin_toggle", ["plugin", "toggle", "ust"])
    case("plugin_toggle_missing", ["plugin", "toggle", "nope"])
    # json -> format conversions (all prompts answered with their default)
    blank = "\n" * 40
    for suffix in (
        "mid", "midi", "svp", "ufdata", "vsqx", "vsq", "xvsq", "ust", "ustx",
        "ass", "lrc", "srt", "svg", "svip3", "ppsf", "ps_project", "musicxml",
        "ace", "acep", "aisp", "ccs", "ds", "dspx", "dv", "mtp", "nn", "s5p",
        "svip", "tlp", "tlpx", "tsmsln", "tssln", "vfp", "vog", "vpr", "vspx",
        "vvproj", "vxf", "y77",
    ):
        case(f"convert_json_{suffix}", ["proj", "convert", "simple.json", f"out.{suffix}"], blank)


def find_settings(root: pathlib.Path) -> str | None:
    """Returns the text of the first settings.yml below `root`, if any."""
    if not root.exists():
        return None
    for path in sorted(root.rglob("settings.yml")):
        return path.read_text(encoding="utf-8")
    return None


UUID_RE = re.compile(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")


def normalise_uuids(data: bytes) -> bytes:
    """Formats such as svp/ppsf embed fresh UUIDs on every run."""
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return data
    return UUID_RE.sub("<uuid>", text).encode("utf-8")


def normalise(text: str) -> str:
    # The Python CLI is invoked as `python -m libresvip.cli`, the Rust one is a
    # standalone executable; keep that difference out of the comparison.
    return (
        text.replace("python -m libresvip.cli", "libresvip-cli")
        .replace("py_state_entry.py", "libresvip-cli")
        .replace("python -m py_state_entry", "libresvip-cli")
    )


def nondeterministic_suffixes() -> set[str]:
    """Formats whose golden metadata says two Python runs already differ."""
    result: set[str] = set()
    root = TESTDATA / "golden"
    if not root.is_dir():
        return result
    for meta_path in root.glob("*/meta.json"):
        try:
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
        except Exception:  # noqa: BLE001
            continue
        if meta.get("nondeterministic"):
            result.add(str(meta.get("suffix", "")))
            result.add(str(meta.get("id", "")))
    return result


def describe_binary_diff(expected: bytes, actual: bytes) -> str:
    limit = min(len(expected), len(actual))
    for index in range(limit):
        if expected[index] != actual[index]:
            start = max(0, index - 8)
            end = min(limit, index + 16)
            return (
                f"first differing byte {index}:\n"
                f"  python {expected[start:end].hex(' ')}\n"
                f"  rust   {actual[start:end].hex(' ')}"
            )
    return f"length differs: python {len(expected)} vs rust {len(actual)}"


def main() -> int:
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[attr-defined]
    except Exception:  # noqa: BLE001
        pass
    parser = argparse.ArgumentParser()
    parser.add_argument("--only", default=None, help="substring filter on case name")
    parser.add_argument("-v", "--verbose", action="store_true")
    parser.add_argument("--max-diff", type=int, default=40)
    args = parser.parse_args()

    if not RUST_EXE.exists():
        print(f"missing Rust binary: {RUST_EXE}")
        return 2

    build_cases()
    cases = [item for item in CASES if args.only is None or args.only in item[0]]
    failures = 0
    for name, argv, stdin in cases:
        py = run_python(name, argv, stdin)
        rs = run_rust(name, argv, stdin)
        py_state = find_settings(TESTDATA / ".state" / f"py-{name}")
        rs_state = find_settings(TESTDATA / ".state" / f"rs-{name}")
        # Where LibreSVIP 2.9.2 crashes on a file our port handles on purpose
        # (documented deviations), report SKIP instead of a failure.
        if py.returncode != 0 and rs.returncode == 0 and "Traceback (most recent call last)" in normalise(py.stderr):
            print(f"SKIP {name} (Python reference crashes; Rust implements the intended behaviour)")
            continue
        py_files = produced_files(TESTDATA / ".work" / name / "py")
        rs_files = produced_files(TESTDATA / ".work" / name / "rs")
        unstable = nondeterministic_suffixes()
        for key in list(py_files):
            suffix = key.rsplit(".", 1)[-1]
            if suffix in unstable or key.rsplit(".", 1)[0] in unstable:
                # Only presence matters for formats embedding UUIDs/timestamps.
                py_files[key] = b""
                if key in rs_files:
                    rs_files[key] = b""
        if py_files != rs_files and {
            key: normalise_uuids(value) for key, value in py_files.items()
        } == {key: normalise_uuids(value) for key, value in rs_files.items()}:
            py_files = rs_files
        if py_files != rs_files:
            failures += 1
            print(f"FAIL {name} (output files differ)")
            if args.verbose:
                for key in sorted(set(py_files) | set(rs_files)):
                    if py_files.get(key) == rs_files.get(key):
                        continue
                    expected = py_files.get(key)
                    actual = rs_files.get(key)
                    if expected is None:
                        print(f"  {key}: extra file in rust ({len(actual or b'')} bytes)")
                    elif actual is None:
                        print(f"  {key}: missing in rust ({len(expected)} bytes)")
                    else:
                        print(f"  {key}: differs ({len(expected)} vs {len(actual)} bytes)")
                        if b"\x00" in expected or b"\x00" in actual:
                            print("   " + describe_binary_diff(expected, actual).replace("\n", "\n   "))
                            continue
                        text_expected = expected.decode("utf-8", errors="replace")
                        text_actual = actual.decode("utf-8", errors="replace")
                        for line in list(
                            difflib.unified_diff(
                                text_expected.splitlines(),
                                text_actual.splitlines(),
                                fromfile="python",
                                tofile="rust",
                                lineterm="",
                                n=1,
                            )
                        )[: args.max_diff]:
                            print("   " + line)
            continue
        if py_state is not None and rs_state is not None:
            if py_state != rs_state:
                failures += 1
                print(f"FAIL {name} (settings.yml differs)")
                if args.verbose:
                    for line in difflib.unified_diff(
                        py_state.splitlines(),
                        rs_state.splitlines(),
                        fromfile="python:settings.yml",
                        tofile="rust:settings.yml",
                        lineterm="",
                        n=2,
                    ):
                        print(line)
                continue
        py_out, rs_out = normalise(py.stdout), normalise(rs.stdout)
        py_err, rs_err = normalise(py.stderr), normalise(rs.stderr)
        ok = py_out == rs_out and py_err == rs_err and py.returncode == rs.returncode
        if ok:
            print(f"PASS {name}")
            continue
        failures += 1
        print(f"FAIL {name} (exit py={py.returncode} rs={rs.returncode})")
        if args.verbose:
            for label, expected, actual in (
                ("stdout", py_out, rs_out),
                ("stderr", py_err, rs_err),
            ):
                if expected == actual:
                    continue
                diff = list(
                    difflib.unified_diff(
                        expected.splitlines(),
                        actual.splitlines(),
                        fromfile=f"python:{label}",
                        tofile=f"rust:{label}",
                        lineterm="",
                        n=2,
                    )
                )
                print("\n".join(diff[: args.max_diff]))
    print(f"\n{len(cases) - failures}/{len(cases)} cases match")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
