"""Differential check: Rust `libresvip-cli rpc server` vs Python reference.

Starts the Python reference server (`python -m libresvip.cli rpc server`) and the
Rust server (`target/debug/libresvip-cli.exe rpc server`) on free ports, then:

* calls `PluginInfos` for INPUT / OUTPUT / MIDDLEWARE in `zh_CN` and `en_US` and
  compares every response field, including the pydantic-compatible
  `json_schema` (parsed JSON equality; raw byte equality is reported too);
* calls `Convert` for direct / split / merge / middleware / option / error
  cases and compares success flags, warning messages, converted bytes
  (UUID-normalised where the format is nondeterministic) and gRPC statuses;
* checks that `rpc server --help` advertises the same `--host` / `--port`
  options and defaults as the Python CLI.

Run from the workspace root:

    E:\\miniconda\\envs\\study12\\python.exe scripts\\check_rpc.py

Exit code 0 means every case passed.
"""

from __future__ import annotations

import argparse
import asyncio
import copy
import json
import os
import re
import socket
import subprocess
import sys
import tempfile
import time
import traceback
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parent.parent
PY_ROOT = WORKSPACE.parent / "LibreSVIP"
RUST_BIN = WORKSPACE / "target" / "debug" / "libresvip-cli.exe"
TESTDATA = WORKSPACE / "testdata"

# The reference package is a sibling checkout, not installed into this
# interpreter; make its message classes importable for the client side.
sys.path.insert(0, str(PY_ROOT))

from grpclib.client import Channel  # noqa: E402
from grpclib.const import Cardinality, Status
from grpclib.exceptions import GRPCError

from libresvip.cli.rpc.libresvip_pb import (
    ConversionGroup,
    ConversionMode,
    ConversionRequest,
    ConversionResponse,
    PluginCategory,
    PluginInfosRequest,
    PluginInfosResponse,
)
from libresvip.cli.rpc.libresvip_pb_grpc import ProtobufPyCodec

UUID_RE = re.compile(
    rb"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
)

PASS = 0
FAIL = 0
NOTES: list[str] = []


def record(name: str, ok: bool, detail: str = "") -> None:
    global PASS, FAIL
    if ok:
        PASS += 1
        print(f"PASS  {name}")
    else:
        FAIL += 1
        print(f"FAIL  {name}")
        if detail:
            for line in detail.splitlines():
                print(f"      {line}")


def note(message: str) -> None:
    NOTES.append(message)
    print(f"NOTE  {message}")


# ---------------------------------------------------------------------------
# Server lifecycle
# ---------------------------------------------------------------------------


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_port(port: int, process: subprocess.Popen, timeout: float = 120.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"server exited early with code {process.returncode}")
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.25)
            try:
                sock.connect(("127.0.0.1", port))
                return
            except OSError:
                time.sleep(0.2)
    raise RuntimeError(f"server did not listen on port {port} within {timeout}s")


def ensure_winloop_shim(base: Path) -> str | None:
    """The reference CLI imports `winloop` on Windows; provide a tiny shim when
    the interpreter does not ship it (the default asyncio loop works fine)."""
    try:
        import importlib.util

        if importlib.util.find_spec("winloop") is not None:
            return None
    except ModuleNotFoundError:
        pass
    shim_dir = base / "winloop_shim"
    shim_dir.mkdir(parents=True, exist_ok=True)
    (shim_dir / "winloop.py").write_text(
        "import asyncio\n\ndef new_event_loop():\n    return asyncio.new_event_loop()\n",
        encoding="utf-8",
    )
    return str(shim_dir)


class Servers:
    def __init__(self) -> None:
        self.logs: dict[str, Path] = {}
        self.processes: list[subprocess.Popen] = []
        self.temp = tempfile.TemporaryDirectory(prefix="libresvip-rpc-check-")

    def start(
        self,
        name: str,
        command: list[str],
        cwd: Path,
        env: dict[str, str],
        port: int,
    ) -> int:
        log = Path(self.temp.name) / f"{name}.log"
        self.logs[name] = log
        handle = open(log, "wb")
        process = subprocess.Popen(
            command,
            cwd=str(cwd),
            env=env,
            stdout=handle,
            stderr=subprocess.STDOUT,
        )
        process._log_handle = handle  # type: ignore[attr-defined]
        self.processes.append(process)
        wait_for_port(port, process)
        return port

    def dump_logs(self) -> None:
        for name, path in self.logs.items():
            try:
                text = path.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            if text.strip():
                print(f"--- {name} server output (tail) ---")
                lines = text.splitlines()
                for line in lines[-40:]:
                    print(line)

    def stop(self) -> None:
        for process in self.processes:
            if process.poll() is None:
                process.terminate()
        for process in self.processes:
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=15)
            handle = getattr(process, "_log_handle", None)
            if handle is not None:
                handle.close()
        self.temp.cleanup()


def python_env() -> dict[str, str]:
    env = os.environ.copy()
    path_entries = [str(PY_ROOT)]
    shim = ensure_winloop_shim(Path(tempfile.gettempdir()) / "libresvip_winloop_shim")
    if shim:
        path_entries.insert(0, shim)
        note("winloop is not installed for the reference interpreter; using an asyncio shim")
    existing = env.get("PYTHONPATH")
    if existing:
        path_entries.append(existing)
    env["PYTHONPATH"] = os.pathsep.join(path_entries)
    return env


# ---------------------------------------------------------------------------
# gRPC calls
# ---------------------------------------------------------------------------


async def call(port: int, path: str, request_type, response_type, request):
    async with Channel("127.0.0.1", port, codec=ProtobufPyCodec()) as channel:
        async with channel.request(
            path, Cardinality.UNARY_UNARY, request_type, response_type
        ) as stream:
            await stream.send_message(request)
            return await stream.recv_message()


async def call_both(ports: dict[str, int], path: str, request_type, response_type, request):
    py = await call(ports["python"], path, request_type, response_type, request)
    rs = await call(ports["rust"], path, request_type, response_type, request)
    return py, rs


async def call_expect_error(port: int, request: ConversionRequest) -> tuple[Status | None, str]:
    try:
        await call(
            port,
            "/LibreSVIP.Conversion/Convert",
            ConversionRequest,
            ConversionResponse,
            request,
        )
    except GRPCError as error:
        status = getattr(error, "status", None)
        message = str(getattr(error, "message", error))
        return status, message
    return None, ""


# ---------------------------------------------------------------------------
# PluginInfos comparison
# ---------------------------------------------------------------------------


def first_difference(left: str, right: str) -> str:
    limit = min(len(left), len(right))
    for index in range(limit):
        if left[index] != right[index]:
            start = max(0, index - 80)
            return (
                f"first difference at offset {index}\n"
                f"python: ...{left[start:index + 120]}...\n"
                f"rust:   ...{right[start:index + 120]}..."
            )
    return f"length differs (python {len(left)}, rust {len(right)})"


def compare_plugin_infos(
    category: PluginCategory, category_name: str, language: str, py, rs
) -> None:
    name = f"PluginInfos {category_name} {language}"
    py_values = list(py.values)
    rs_values = list(rs.values)
    problems: list[str] = []
    if len(py_values) != len(rs_values):
        problems.append(f"value count: python {len(py_values)}, rust {len(rs_values)}")
    py_ids = [item.identifier for item in py_values]
    rs_ids = [item.identifier for item in rs_values]
    if py_ids != rs_ids:
        problems.append(f"identifier order differs:\n  python {py_ids}\n  rust   {rs_ids}")
    for index, (left, right) in enumerate(zip(py_values, rs_values)):
        label = f"[{index}] {left.identifier}"
        for field in (
            "identifier",
            "name",
            "version",
            "description",
            "author",
            "website",
            "file_format",
            "icon_base64",
        ):
            if getattr(left, field) != getattr(right, field):
                problems.append(
                    f"{label} field {field}:\n"
                    f"    python {getattr(left, field)!r}\n"
                    f"    rust   {getattr(right, field)!r}"
                )
        if list(left.suffixes) != list(right.suffixes):
            problems.append(
                f"{label} suffixes: python {list(left.suffixes)!r}, "
                f"rust {list(right.suffixes)!r}"
            )
        try:
            py_schema = json.loads(left.json_schema)
        except json.JSONDecodeError as error:
            problems.append(f"{label} python json_schema is invalid JSON: {error}")
            continue
        try:
            rs_schema = json.loads(right.json_schema)
        except json.JSONDecodeError as error:
            problems.append(f"{label} rust json_schema is invalid JSON: {error}")
            continue
        if py_schema != rs_schema:
            problems.append(
                f"{label} json_schema differs (parsed):\n"
                f"{first_difference(left.json_schema, right.json_schema)}"
            )
        elif left.json_schema != right.json_schema:
            problems.append(
                f"{label} json_schema parsed equal but raw bytes differ:\n"
                f"{first_difference(left.json_schema, right.json_schema)}"
            )
    record(name, not problems, "\n".join(problems))


async def check_plugin_infos(ports: dict[str, int]) -> None:
    categories = [
        (PluginCategory.INPUT, "input"),
        (PluginCategory.OUTPUT, "output"),
        (PluginCategory.MIDDLEWARE, "middleware"),
    ]
    for category, category_name in categories:
        for language in ("en_US", "zh_CN", "ja_JP"):
            request = PluginInfosRequest(category=category, language=language)
            py, rs = await call_both(
                ports,
                "/LibreSVIP.Conversion/PluginInfos",
                PluginInfosRequest,
                PluginInfosResponse,
                request,
            )
            compare_plugin_infos(category, category_name, language, py, rs)


# ---------------------------------------------------------------------------
# Convert comparison
# ---------------------------------------------------------------------------


def normalize_file_bytes(label: str, data: bytes) -> bytes:
    # SVP embeds random UUIDs for notes/groups; golden metadata marks the format
    # as nondeterministic.
    if "svp" in label:
        return UUID_RE.sub(b"<uuid>", data)
    return data


def compare_group_results(label: str, py_response, rs_response) -> list[str]:
    problems: list[str] = []
    py_groups = dict(py_response.group_results)
    rs_groups = dict(rs_response.group_results)
    if set(py_groups) != set(rs_groups):
        problems.append(
            f"group ids: python {sorted(py_groups)}, rust {sorted(rs_groups)}"
        )
    for group_id in sorted(set(py_groups) & set(rs_groups)):
        left = py_groups[group_id]
        right = rs_groups[group_id]
        if left.success != right.success:
            problems.append(
                f"{group_id}: success python {left.success}, rust {right.success}"
            )
        if list(left.warning_messages) != list(right.warning_messages):
            problems.append(
                f"{group_id}: warnings\n    python {list(left.warning_messages)!r}\n"
                f"    rust   {list(right.warning_messages)!r}"
            )
        if left.success:
            if left.error_message or right.error_message:
                problems.append(
                    f"{group_id}: successful result carries an error message "
                    f"(python {left.error_message!r}, rust {right.error_message!r})"
                )
            left_files = [normalize_file_bytes(label, item) for item in left.file_contents]
            right_files = [normalize_file_bytes(label, item) for item in right.file_contents]
            if len(left_files) != len(right_files):
                problems.append(
                    f"{group_id}: file count python {len(left_files)}, rust {len(right_files)}"
                )
            for index, (py_file, rs_file) in enumerate(zip(left_files, right_files)):
                if py_file != rs_file:
                    problems.append(
                        f"{group_id}: file {index} bytes differ "
                        f"(python {len(py_file)}, rust {len(rs_file)})\n"
                        f"    python prefix {py_file[:160]!r}\n"
                        f"    rust   prefix {rs_file[:160]!r}"
                    )
        else:
            # Python's error_message is a full traceback and cannot be compared
            # verbatim; require both sides to report a failure message.
            if not left.error_message:
                problems.append(f"{group_id}: python reported failure without error_message")
            if not right.error_message:
                problems.append(f"{group_id}: rust reported failure without error_message")
    return problems


def build_two_track_project() -> bytes:
    project = json.loads((TESTDATA / "simple.json").read_bytes().decode("utf-8"))
    second = copy.deepcopy(project["TrackList"][0])
    second["Title"] = "Track 2"
    project["TrackList"] = [project["TrackList"][0], second]
    return json.dumps(project, ensure_ascii=False).encode("utf-8")


def build_short_note_project() -> bytes:
    project = json.loads((TESTDATA / "simple.json").read_bytes().decode("utf-8"))
    project["TrackList"][0]["NoteList"][0]["Length"] = 1
    return json.dumps(project, ensure_ascii=False).encode("utf-8")


def build_many_short_notes_project() -> bytes:
    project = json.loads((TESTDATA / "simple.json").read_bytes().decode("utf-8"))
    for note in project["TrackList"][0]["NoteList"]:
        note["Length"] = 1
    return json.dumps(project, ensure_ascii=False).encode("utf-8")


def build_instrumental_only_project() -> bytes:
    project = json.loads((TESTDATA / "simple.json").read_bytes().decode("utf-8"))
    project["TrackList"] = [
        track for track in project["TrackList"] if track["Type"] == "Instrumental"
    ]
    return json.dumps(project, ensure_ascii=False).encode("utf-8")


def build_ust_without_tempo() -> bytes:
    data = (TESTDATA / "golden" / "ust" / "dump.ust").read_bytes()
    return re.sub(rb"(?m)^Tempo=.*\r?\n", b"", data)


def conversion_cases() -> list[dict]:
    simple = (TESTDATA / "simple.json").read_bytes()
    tiny = (TESTDATA / "simple_singing.json").read_bytes()
    two_tracks = build_two_track_project()
    short_note = build_short_note_project()
    many_short_notes = build_many_short_notes_project()
    instrumental_only = build_instrumental_only_project()
    ust_without_tempo = build_ust_without_tempo()
    large_project = (TESTDATA / "bench_large.json").read_bytes()
    return [
        {
            "name": "Convert json->ust direct",
            "input_format": "json",
            "output_format": "ust",
            "groups": [("direct-ust", [simple])],
        },
        {
            "name": "Convert json->svp direct (UUID normalised)",
            "input_format": "json",
            "output_format": "svp",
            "groups": [("direct-svp", [simple])],
        },
        {
            "name": "Convert json->json split max_track_count=1",
            "input_format": "json",
            "output_format": "json",
            "mode": ConversionMode.SPLIT,
            "max_track_count": 1,
            "groups": [("split-1", [two_tracks])],
        },
        {
            "name": "Convert json->json split max_track_count=100 (too-large group)",
            "input_format": "json",
            "output_format": "json",
            "mode": ConversionMode.SPLIT,
            "max_track_count": 100,
            "groups": [("split-100", [two_tracks])],
        },
        {
            "name": "Convert json->json split max_track_count=0",
            "input_format": "json",
            "output_format": "json",
            "mode": ConversionMode.SPLIT,
            "max_track_count": 0,
            "groups": [("split-0", [simple])],
        },
        {
            "name": "Convert json->json split max_track_count=-1 (RPC error)",
            "input_format": "json",
            "output_format": "json",
            "mode": ConversionMode.SPLIT,
            "max_track_count": -1,
            "groups": [("split-neg", [simple])],
            "expect_error": True,
        },
        {
            "name": "Convert json->json merge 2 files",
            "input_format": "json",
            "output_format": "json",
            "mode": ConversionMode.MERGE,
            "groups": [("merge-2", [simple, tiny])],
        },
        {
            "name": "Convert json->json merge 1 file (RPC error)",
            "input_format": "json",
            "output_format": "json",
            "mode": ConversionMode.MERGE,
            "groups": [("merge-1", [simple])],
            "expect_error": True,
        },
        {
            "name": "Convert json->json empty group (RPC error)",
            "input_format": "json",
            "output_format": "json",
            "groups": [("empty", [])],
            "expect_error": True,
        },
        {
            "name": "Convert invalid input format (RPC error)",
            "input_format": "definitely-not-a-format",
            "output_format": "json",
            "groups": [("bad-in", [simple])],
            "expect_error": True,
        },
        {
            "name": "Convert invalid output format (RPC error)",
            "input_format": "json",
            "output_format": "definitely-not-a-format",
            "groups": [("bad-out", [simple])],
            "expect_error": True,
        },
        {
            "name": "Convert write-only plugin as input (RPC error)",
            "input_format": "ass",
            "output_format": "json",
            "groups": [("ass-in", [simple])],
            "expect_error": True,
        },
        {
            "name": "Convert read-only plugin as output (RPC error)",
            "input_format": "json",
            "output_format": "vshp",
            "groups": [("vshp-out", [simple])],
            "expect_error": True,
        },
        {
            "name": "Convert json->json indented output options",
            "input_format": "json",
            "output_format": "json",
            "output_options": '{"indented": true}',
            "groups": [("indented", [simple])],
        },
        {
            "name": "Convert json->json middleware pitch_shift",
            "input_format": "json",
            "output_format": "json",
            "middleware_options": {"pitch_shift": '{"key": 12}'},
            "groups": [("middleware", [simple])],
        },
        {
            "name": "Convert json->json middleware invalid option JSON",
            "input_format": "json",
            "output_format": "json",
            "middleware_options": {"pitch_shift": "not json"},
            "groups": [("middleware-bad", [simple])],
        },
        {
            "name": "Convert json->json two middlewares in request order",
            "input_format": "json",
            "output_format": "json",
            "middleware_options": {
                "pitch_shift": '{"key": 2}',
                "project_zoom": '{"factor": "2/1"}',
            },
            "groups": [("middleware-two", [simple])],
        },
        {
            "name": "Convert json->svp output options",
            "input_format": "json",
            "output_format": "svp",
            "output_options": (
                '{"version_compatibility": 182, "vibrato": "always", '
                '"language_override": "japanese"}'
            ),
            "groups": [("svp-options", [simple])],
        },
        {
            "name": "Convert json->svp invalid enum option falls back to defaults",
            "input_format": "json",
            "output_format": "svp",
            "output_options": '{"version_compatibility": 182, "vibrato": "bogus"}',
            "groups": [("svp-bad-options", [simple])],
        },
        {
            "name": "Convert json->svp input option string coercion",
            "input_format": "json",
            "output_format": "svp",
            "input_options": '{"import_instrumental_track": "no", "pitch": "full"}',
            "groups": [("svp-input-options", [simple])],
        },
        {
            "name": "Convert json->ust output encoding option",
            "input_format": "json",
            "output_format": "ust",
            "output_options": '{"encoding": "utf-8", "version": 1.2}',
            "groups": [("ust-options", [simple])],
        },
        {
            "name": "Convert json->midi suffix resolution",
            "input_format": "json",
            "output_format": "midi",
            "groups": [("midi-suffix", [simple])],
        },
        {
            "name": "Convert two groups both successful",
            "input_format": "json",
            "output_format": "json",
            "groups": [("multi-a", [simple]), ("multi-b", [tiny])],
        },
        {
            "name": "Convert two groups with one empty group (RPC error)",
            "input_format": "json",
            "output_format": "json",
            "groups": [("multi-c", [simple]), ("multi-d", [])],
            "expect_error": True,
        },
        {
            "name": "Convert nested group id (created like memory:// mkdir)",
            "input_format": "json",
            "output_format": "json",
            "groups": [("nested/group", [simple])],
        },
        {
            "name": "Convert root-relative group id",
            "input_format": "json",
            "output_format": "json",
            "groups": [("/absolute", [simple])],
        },
        {
            "name": "Convert json->json unknown middleware",
            "input_format": "json",
            "output_format": "json",
            "middleware_options": {"no-such-middleware": "{}"},
            "groups": [("middleware-unknown", [simple])],
        },
        {
            "name": "Convert json->svip short note warning (SPLIT captures dump warnings)",
            "input_format": "json",
            "output_format": "svip",
            "mode": ConversionMode.SPLIT,
            "max_track_count": 1,
            "groups": [("warning", [short_note])],
        },
        {
            "name": "Convert json->svip direct dump warnings are not in the response",
            "input_format": "json",
            "output_format": "svip",
            "groups": [("warning-direct", [short_note])],
        },
        {
            "name": "Convert json->svip multiple warnings in one group",
            "input_format": "json",
            "output_format": "svip",
            "mode": ConversionMode.SPLIT,
            "max_track_count": 1,
            "groups": [("warnings-many", [many_short_notes])],
        },
        {
            "name": "Convert ust->json load warning (missing tempo)",
            "input_format": "ust",
            "output_format": "json",
            "groups": [("ust-warning", [ust_without_tempo])],
        },
        {
            "name": "Convert split max_track_count=0 without singing tracks (RPC error)",
            "input_format": "json",
            "output_format": "json",
            "mode": ConversionMode.SPLIT,
            "max_track_count": 0,
            "groups": [("split-instrumental", [instrumental_only])],
            "expect_error": True,
        },
        {
            "name": "Convert json->json large project",
            "input_format": "json",
            "output_format": "json",
            "groups": [("large", [large_project])],
        },
        {
            "name": "Convert json bad bytes (per-group failure)",
            "input_format": "json",
            "output_format": "json",
            "groups": [("bad-bytes", [b"not json"])],
        },
        {
            "name": "Convert empty groups list",
            "input_format": "json",
            "output_format": "json",
            "groups": [],
        },
    ]


async def check_convert(ports: dict[str, int]) -> None:
    for case in conversion_cases():
        request = ConversionRequest(
            input_format=case["input_format"],
            output_format=case["output_format"],
            mode=case.get("mode", ConversionMode.DIRECT),
            max_track_count=case.get("max_track_count", 0),
            groups=[
                ConversionGroup(group_id=group_id, file_contents=contents)
                for group_id, contents in case["groups"]
            ],
            input_options=case.get("input_options", ""),
            output_options=case.get("output_options", ""),
            middleware_options=case.get("middleware_options", {}),
        )
        name = case["name"]
        if case.get("expect_error"):
            py_status, py_message = await call_expect_error(ports["python"], request)
            rs_status, rs_message = await call_expect_error(ports["rust"], request)
            problems = []
            if py_status is None:
                problems.append("python did not raise")
            if rs_status is None:
                problems.append("rust did not raise")
            if py_status is not None and rs_status is not None and py_status != rs_status:
                problems.append(f"status python {py_status}, rust {rs_status}")
            if rs_status is not None and not rs_message:
                problems.append("rust error carried no message")
            record(name, not problems, "\n".join(problems))
            continue
        try:
            py_response, rs_response = await call_both(
                ports,
                "/LibreSVIP.Conversion/Convert",
                ConversionRequest,
                ConversionResponse,
                request,
            )
        except GRPCError as error:
            record(name, False, f"unexpected gRPC error: {error!r}")
            continue
        problems = compare_group_results(name, py_response, rs_response)
        record(name, not problems, "\n".join(problems))


# ---------------------------------------------------------------------------
# CLI help comparison
# ---------------------------------------------------------------------------


def check_help() -> None:
    python = subprocess.run(
        [sys.executable, "-m", "libresvip.cli", "rpc", "server", "--help"],
        cwd=str(PY_ROOT),
        env=python_env(),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=120,
    )
    rust = subprocess.run(
        [str(RUST_BIN), "rpc", "server", "--help"],
        cwd=str(WORKSPACE),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=120,
    )
    if python.returncode != 0:
        record("rpc server --help", False, python.stderr[-2000:])
        return
    if rust.returncode != 0:
        record("rpc server --help", False, rust.stderr[-2000:])
        return
    problems = []
    for option, default in (("--host", "127.0.0.1"), ("--port", "15150")):
        if option not in python.stdout:
            problems.append(f"python help lacks {option}")
        if option not in rust.stdout:
            problems.append(f"rust help lacks {option}")
        if f"default: {default}" not in python.stdout:
            problems.append(f"python help lacks default {default} for {option}")
        if f"default: {default}" not in rust.stdout:
            problems.append(f"rust help lacks default {default} for {option}")
    record("rpc server --help arguments", not problems, "\n".join(problems))


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------


def build_rust() -> None:
    cargo = os.path.join(os.path.expanduser("~"), ".cargo", "bin", "cargo.exe")
    if not os.path.exists(cargo):
        cargo = os.path.join(os.path.expanduser("~"), ".cargo", "bin", "cargo")
    if not os.path.exists(cargo):
        cargo = "cargo"
    result = subprocess.run(
        [cargo, "build", "-p", "libresvip-cli"],
        cwd=str(WORKSPACE),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode != 0:
        print(result.stdout[-4000:])
        print(result.stderr[-4000:])
        raise SystemExit("cargo build failed")


async def async_main(ports: dict[str, int]) -> None:
    await check_plugin_infos(ports)
    await check_convert(ports)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-build", action="store_true", help="skip cargo build")
    args = parser.parse_args()

    if not PY_ROOT.is_dir():
        raise SystemExit(f"Python reference tree not found: {PY_ROOT}")
    if not args.no_build:
        build_rust()
    if not RUST_BIN.exists():
        raise SystemExit(f"Rust binary not found: {RUST_BIN}")

    check_help()

    servers = Servers()
    ports = {"python": free_port(), "rust": free_port()}
    while ports["rust"] == ports["python"]:
        ports["rust"] = free_port()
    try:
        servers.start(
            "python",
            [
                sys.executable,
                "-m",
                "libresvip.cli",
                "rpc",
                "server",
                "--host",
                "127.0.0.1",
                "--port",
                str(ports["python"]),
            ],
            PY_ROOT,
            python_env(),
            ports["python"],
        )
        servers.start(
            "rust",
            [
                str(RUST_BIN),
                "rpc",
                "server",
                "--host",
                "127.0.0.1",
                "--port",
                str(ports["rust"]),
            ],
            WORKSPACE,
            os.environ.copy(),
            ports["rust"],
        )
        asyncio.run(async_main(ports))
    except Exception:
        traceback.print_exc()
        global FAIL
        FAIL += 1
    finally:
        if FAIL:
            servers.dump_logs()
        servers.stop()

    print()
    print(f"SUMMARY: {PASS} passed, {FAIL} failed")
    for message in NOTES:
        print(f"note: {message}")
    return 0 if FAIL == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
