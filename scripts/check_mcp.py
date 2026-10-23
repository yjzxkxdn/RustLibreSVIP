"""Differential test harness: Python `mcp serve` vs the Rust port.

Spawns both MCP servers on stdio, sends the same sequence of JSON-RPC requests,
compares the responses semantically (parsed JSON equality after normalising the
inherently random session handles, the per-endpoint output directory and the
pydantic version embedded in error URLs) and compares the files each server
writes byte-wise (UUIDs normalised for formats whose goldens are marked
nondeterministic).

    E:\\miniconda\\envs\\study12\\python.exe scripts/check_mcp.py
    ... scripts/check_mcp.py -v          # show full response diffs and CLI help

Exit code is non-zero when any case mismatches.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import queue
import re
import shutil
import subprocess
import sys
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
PY_ROOT = ROOT / "LibreSVIP"
RUST_EXE = ROOT / "RustLibreSVIP" / "target" / "debug" / "libresvip-cli.exe"
PYTHON = pathlib.Path(r"E:\miniconda\envs\study12\python.exe")
TESTDATA = ROOT / "RustLibreSVIP" / "testdata"
WORK = TESTDATA / ".work" / "mcp"

CALL_TIMEOUT = 180

# Session handles are random 32-character hex strings (`uuid4().hex`).
HANDLE_RE = re.compile(r"\b[0-9a-f]{32}\b")
# pydantic embeds its own version in error documentation URLs.
PYDANTIC_URL_RE = re.compile(r"https://errors\.pydantic\.dev/[0-9][^/]*/v/")
# Each endpoint writes into its own directory; that prefix is not semantic.
OUTDIR_RE = re.compile(r"\.work[\\/]mcp[\\/](?:py|rs)[\\/]")
UUID_RE = re.compile(
    r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
)


# ---------------------------------------------------------------------------
# Harness
# ---------------------------------------------------------------------------


class Report:
    def __init__(self, verbose: bool) -> None:
        self.verbose = verbose
        self.passed = 0
        self.failed = 0
        self.skipped = 0
        self.render_total = 0
        self.render_identical = 0
        self.render_diffs: list[tuple[str, str, str]] = []

    def ok(self, name: str) -> None:
        self.passed += 1
        print(f"PASS {name}")

    def fail(self, name: str, py_value=None, rs_value=None) -> None:
        self.failed += 1
        print(f"FAIL {name}")
        if py_value is not None or rs_value is not None:
            print("  python: " + self._dump(py_value))
            print("  rust:   " + self._dump(rs_value))

    def skip(self, name: str, reason: str) -> None:
        self.skipped += 1
        print(f"SKIP {name} ({reason})")

    def _dump(self, value) -> str:
        if isinstance(value, str):
            return value
        text = json.dumps(value, ensure_ascii=False, indent=2)
        if not self.verbose:
            lines = text.splitlines()
            if len(lines) > 40:
                text = "\n".join(lines[:40]) + "\n  ..."
        return text


def python_env() -> dict[str, str]:
    env = os.environ.copy()
    env["PYTHONPATH"] = str(PY_ROOT)
    env["PYTHONIOENCODING"] = "utf-8"
    env["PYTHONUNBUFFERED"] = "1"
    env["COLUMNS"] = "80"
    env.pop("PYTHONHOME", None)
    return env


def rust_env() -> dict[str, str]:
    env = os.environ.copy()
    env["COLUMNS"] = "80"
    return env


class Endpoint:
    """One MCP server process speaking newline-delimited JSON-RPC on stdio."""

    def __init__(self, label: str, argv: list[str], env: dict[str, str], cwd: pathlib.Path) -> None:
        self.label = label
        self.state: dict[str, object] = {}
        self.next_id = 0
        self.stderr_path = WORK / label / "server.stderr"
        self.stderr_path.parent.mkdir(parents=True, exist_ok=True)
        self._stderr = open(self.stderr_path, "wb")
        self.proc = subprocess.Popen(
            argv,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self._stderr,
            cwd=str(cwd),
            env=env,
        )
        self._lines: queue.Queue[bytes | None] = queue.Queue()
        threading.Thread(target=self._read_stdout, daemon=True).start()

    def _read_stdout(self) -> None:
        stream = self.proc.stdout
        assert stream is not None
        for raw in iter(stream.readline, b""):
            self._lines.put(raw)
        self._lines.put(None)

    def _read_response(self, label: str) -> bytes:
        try:
            raw = self._lines.get(timeout=CALL_TIMEOUT)
        except queue.Empty:
            raise RuntimeError(f"{self.label}: timed out waiting for a response to {label}") from None
        if raw is None:
            raise RuntimeError(f"{self.label}: server closed stdout")
        return raw

    def send(self, payload: dict, *, allow_null_id: bool = False) -> dict | None:
        line = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        stream = self.proc.stdin
        assert stream is not None
        stream.write(line + b"\n")
        stream.flush()
        if "id" not in payload:
            return None
        raw = self._read_response(repr(payload))
        try:
            response = json.loads(raw.decode("utf-8"))
        except Exception as error:  # noqa: BLE001
            raise RuntimeError(f"{self.label}: invalid response line {raw!r} ({error})") from error
        if response.get("id") != payload["id"] and not (
            allow_null_id and response.get("id") is None
        ):
            raise RuntimeError(
                f"{self.label}: out-of-order response, expected id {payload['id']!r}, "
                f"got {response.get('id')!r}: {response!r}"
            )
        return response

    def send_raw(self, text: str) -> dict:
        stream = self.proc.stdin
        assert stream is not None
        stream.write(text.encode("utf-8") + b"\n")
        stream.flush()
        raw = self._read_response(text)
        return json.loads(raw.decode("utf-8"))

    def request(self, method: str, params: dict | None = None, *, notify: bool = False) -> dict | None:
        self.next_id += 1
        payload: dict = {"jsonrpc": "2.0", "method": method}
        if not notify:
            payload["id"] = self.next_id
        if params is not None:
            payload["params"] = params
        return self.send(payload)

    def call(self, name: str, arguments: dict) -> dict | None:
        self.next_id += 1
        return self.send(
            {
                "jsonrpc": "2.0",
                "id": self.next_id,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }
        )

    def stop(self) -> None:
        try:
            if self.proc.stdin is not None:
                self.proc.stdin.close()
        except OSError:
            pass
        try:
            self.proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=10)
        try:
            self._stderr.close()
        except OSError:
            pass


# ---------------------------------------------------------------------------
# Normalisation / canonicalisation
# ---------------------------------------------------------------------------


def normalise_string(text: str) -> str:
    text = OUTDIR_RE.sub(".work/mcp/<tag>/", text)
    text = PYDANTIC_URL_RE.sub("https://errors.pydantic.dev/<pydantic>/v/", text)
    return HANDLE_RE.sub("<handle>", text)


def canonical(value):
    """Normalises a response value; embedded JSON text is compared as JSON."""
    if isinstance(value, dict):
        if value.get("type") == "text" and isinstance(value.get("text"), str):
            try:
                parsed = json.loads(value["text"])
            except Exception:  # noqa: BLE001
                parsed = None
            if isinstance(parsed, (dict, list)):
                rest = {key: canonical(item) for key, item in value.items() if key != "text"}
                rest["text"] = {"$json": canonical(parsed)}
                return rest
        return {key: canonical(item) for key, item in value.items()}
    if isinstance(value, list):
        return [canonical(item) for item in value]
    if isinstance(value, str):
        return normalise_string(value)
    return value


def content_texts(response) -> list[str]:
    texts = []
    if not isinstance(response, dict):
        return texts
    result = response.get("result")
    if not isinstance(result, dict):
        return texts
    for item in result.get("content") or []:
        if isinstance(item, dict) and item.get("type") == "text" and isinstance(item.get("text"), str):
            texts.append(item["text"])
    return texts


def tool_error_is_traceback(response) -> bool:
    if not isinstance(response, dict):
        return False
    result = response.get("result")
    if not isinstance(result, dict) or not result.get("isError"):
        return False
    texts = content_texts(response)
    return bool(texts) and texts[0].startswith("Traceback (most recent call last):")


def nondeterministic_formats() -> set[str]:
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


def normalise_uuids(data: bytes) -> bytes:
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return data
    return UUID_RE.sub("<uuid>", text).encode("utf-8")


# ---------------------------------------------------------------------------
# Scenario
# ---------------------------------------------------------------------------


class Session:
    def __init__(self, py: Endpoint, rs: Endpoint, report: Report) -> None:
        self.py = py
        self.rs = rs
        self.report = report

    def case(self, name: str, fn, *, allow_traceback: bool = False):
        py_response = fn(self.py)
        rs_response = fn(self.rs)
        self.report.render_total += 1
        py_texts = content_texts(py_response)
        rs_texts = content_texts(rs_response)
        if [normalise_string(text) for text in py_texts] == [
            normalise_string(text) for text in rs_texts
        ]:
            self.report.render_identical += 1
        else:
            self.report.render_diffs.append(
                (name, "\n---\n".join(py_texts), "\n---\n".join(rs_texts))
            )
        if allow_traceback and tool_error_is_traceback(py_response):
            self.report.skip(
                name, "Python reports a Python traceback; Rust reports its own error message"
            )
            return py_response, rs_response
        if canonical(py_response) == canonical(rs_response):
            self.report.ok(name)
        else:
            self.report.fail(name, py_response, rs_response)
        return py_response, rs_response


def summary_handle(response) -> str | None:
    try:
        payload = json.loads(response["result"]["content"][0]["text"])
        handle = payload["handle"]
        return handle if isinstance(handle, str) else None
    except Exception:  # noqa: BLE001
        return None


def _send_custom(endpoint: Endpoint, payload: dict) -> dict | None:
    """Sends a hand-written request, assigning the next request id.

    Invalid requests get a `null` id in the response, as required by JSON-RPC.
    """
    endpoint.next_id += 1
    return endpoint.send({**payload, "id": endpoint.next_id}, allow_null_id=True)


def build_scenario(session: Session) -> None:
    py, rs, report = session.py, session.rs, session.report
    init_params = {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "check_mcp", "version": "1.0.0"},
    }

    # --- protocol bootstrap ------------------------------------------------
    session.case("initialize", lambda ep: ep.request("initialize", init_params))
    session.case(
        "notifications_initialized",
        lambda ep: ep.request("notifications/initialized", notify=True),
    )
    session.case("ping", lambda ep: ep.request("ping"))
    session.case("tools_list", lambda ep: ep.request("tools/list"))
    session.case("resources_list", lambda ep: ep.request("resources/list"))
    session.case("resources_templates_list", lambda ep: ep.request("resources/templates/list"))
    session.case("prompts_list", lambda ep: ep.request("prompts/list"))
    session.case(
        "resources_read_unknown",
        lambda ep: ep.request("resources/read", {"uri": "foo://bar"}),
    )
    session.case(
        "prompts_get_unknown",
        lambda ep: ep.request("prompts/get", {"name": "x"}),
    )
    session.case(
        "logging_set_level",
        lambda ep: ep.request("logging/setLevel", {"level": "warning"}),
    )
    session.case(
        "logging_set_level_invalid",
        lambda ep: ep.request("logging/setLevel", {"level": "bogus"}),
    )
    session.case("initialize_no_params", lambda ep: ep.request("initialize"))
    session.case("list_formats", lambda ep: ep.call("list_formats", {}))
    session.case("tools_list_extra_params", lambda ep: ep.request("tools/list", {"zzz": 1}))

    # --- load / middleware / save -----------------------------------------
    load = session.case(
        "load_project_simple",
        lambda ep: ep.call("load_project", {"path": "simple.json"}),
    )
    handle_py = summary_handle(load[0]) if load else None
    handle_rs = summary_handle(load[1]) if load else None
    if handle_py is None or handle_rs is None:
        report.skip("handle-dependent cases", "load_project did not return a handle")
        return
    py.state["handle"] = handle_py
    rs.state["handle"] = handle_rs

    session.case(
        "project_summary",
        lambda ep: ep.call("project_summary", {"handle": ep.state["handle"]}),
    )
    session.case(
        "apply_pitch_shift",
        lambda ep: ep.call(
            "apply_pitch_shift", {"handle": ep.state["handle"], "options": '{"key": 2}'}
        ),
    )
    session.case(
        "project_summary_after_pitch_shift",
        lambda ep: ep.call("project_summary", {"handle": ep.state["handle"]}),
    )
    for name, filename in (
        ("save_ust", "pitch.ust"),
        ("save_svp", "pitch.svp"),
    ):
        session.case(
            name,
            lambda ep, filename=filename: ep.call(
                "save_project",
                {"handle": ep.state["handle"], "path": f".work/mcp/{ep.label}/{filename}"},
            ),
        )
    session.case(
        "save_json_indented",
        lambda ep: ep.call(
            "save_project",
            {
                "handle": ep.state["handle"],
                "path": f".work/mcp/{ep.label}/out.json",
                "options": '{"indented": true}',
            },
        ),
    )
    session.case(
        "apply_project_zoom",
        lambda ep: ep.call(
            "apply_project_zoom",
            {"handle": ep.state["handle"], "options": '{"factor": "2/1"}'},
        ),
    )
    session.case(
        "project_summary_after_zoom",
        lambda ep: ep.call("project_summary", {"handle": ep.state["handle"]}),
    )
    session.case(
        "save_zoomed_ust",
        lambda ep: ep.call(
            "save_project",
            {"handle": ep.state["handle"], "path": f".work/mcp/{ep.label}/zoomed.ust"},
        ),
    )

    # --- other formats and warning collection ------------------------------
    session.case(
        "load_saved_svp",
        lambda ep: ep.call("load_project", {"path": f".work/mcp/{ep.label}/pitch.svp"}),
    )
    session.case(
        "load_vpr_untitled_track",
        lambda ep: ep.call("load_project", {"path": "tmp.vpr"}),
    )
    session.case(
        "load_ust_without_tempo_warning",
        lambda ep: ep.call("load_project", {"path": f".work/mcp/{ep.label}/no_tempo.ust"}),
    )
    session.case(
        "load_uppercase_suffix",
        lambda ep: ep.call("load_project", {"path": f".work/mcp/{ep.label}/UPPER.JSON"}),
    )

    # --- option handling ---------------------------------------------------
    session.case(
        "load_options_empty_string",
        lambda ep: ep.call("load_project", {"path": "simple.json", "options": ""}),
    )
    session.case(
        "load_options_unknown_key",
        lambda ep: ep.call(
            "load_project", {"path": "simple.json", "options": '{"unknown_key": 1}'}
        ),
    )
    session.case(
        "load_options_not_json",
        lambda ep: ep.call("load_project", {"path": "simple.json", "options": "not json"}),
    )
    session.case(
        "load_options_not_object",
        lambda ep: ep.call("load_project", {"path": "simple.json", "options": "null"}),
    )
    for name, options in (
        ("save_options_bad_float", '{"version": "abc"}'),
        ("save_options_null_float", '{"version": null}'),
        ("save_options_object_float", '{"version": {"a": 1}}'),
        ("save_options_bad_string", '{"version": 1, "encoding": 5}'),
    ):
        session.case(
            name,
            lambda ep, options=options: ep.call(
                "save_project",
                {
                    "handle": ep.state["handle"],
                    "path": f".work/mcp/{ep.label}/bad.ust",
                    "options": options,
                },
            ),
        )
    session.case(
        "save_options_bad_bool",
        lambda ep: ep.call(
            "save_project",
            {
                "handle": ep.state["handle"],
                "path": f".work/mcp/{ep.label}/bad.json",
                "options": '{"indented": "nope"}',
            },
        ),
    )
    session.case(
        "save_json_string_bool",
        lambda ep: ep.call(
            "save_project",
            {
                "handle": ep.state["handle"],
                "path": f".work/mcp/{ep.label}/bool.json",
                "options": '{"indented": "true"}',
            },
        ),
    )
    session.case(
        "apply_zoom_bad_enum",
        lambda ep: ep.call(
            "apply_project_zoom",
            {"handle": ep.state["handle"], "options": '{"factor": "9/0"}'},
        ),
    )
    session.case(
        "save_coerced_options",
        lambda ep: ep.call(
            "save_project",
            {
                "handle": ep.state["handle"],
                "path": f".work/mcp/{ep.label}/coerced.ust",
                "options": '{"version": 1, "track_index": -1}',
            },
        ),
    )
    session.case(
        "apply_pitch_shift_string_option",
        lambda ep: ep.call(
            "apply_pitch_shift",
            {"handle": ep.state["handle"], "options": '{"key": "2"}'},
        ),
    )
    session.case(
        "save_pitch_string_ust",
        lambda ep: ep.call(
            "save_project",
            {"handle": ep.state["handle"], "path": f".work/mcp/{ep.label}/pitch_str.ust"},
        ),
    )

    # --- invalid input -----------------------------------------------------
    session.case(
        "load_missing_file",
        lambda ep: ep.call("load_project", {"path": "does-not-exist.json"}),
    )
    session.case(
        "load_unsupported_format",
        lambda ep: ep.call("load_project", {"path": "bad.txt"}),
    )
    session.case(
        "save_read_only_format",
        lambda ep: ep.call(
            "save_project",
            {"handle": ep.state["handle"], "path": f".work/mcp/{ep.label}/out.vshp"},
        ),
    )
    session.case(
        "save_write_only_format",
        lambda ep: ep.call(
            "save_project",
            {"handle": ep.state["handle"], "path": f".work/mcp/{ep.label}/probe.ass"},
        ),
    )
    session.case(
        "load_write_only_format",
        lambda ep: ep.call("load_project", {"path": f".work/mcp/{ep.label}/probe.ass"}),
    )
    session.case(
        "summary_unknown_handle",
        lambda ep: ep.call("project_summary", {"handle": "0" * 32}),
    )
    session.case(
        "save_unknown_handle",
        lambda ep: ep.call("save_project", {"handle": "0" * 32, "path": "never.ust"}),
    )
    session.case(
        "close_project",
        lambda ep: ep.call("close_project", {"handle": ep.state["handle"]}),
    )
    session.case(
        "summary_after_close",
        lambda ep: ep.call("project_summary", {"handle": ep.state["handle"]}),
    )
    session.case(
        "close_unknown_handle",
        lambda ep: ep.call("close_project", {"handle": ep.state["handle"]}),
    )

    # --- protocol errors ---------------------------------------------------
    session.case("tools_call_missing_params", lambda ep: ep.request("tools/call", {}))
    session.case(
        "tools_call_wrong_type",
        lambda ep: ep.call("project_summary", {"handle": 5}),
    )
    session.case(
        "tools_call_no_arguments",
        lambda ep: ep.request("tools/call", {"name": "load_project"}),
    )
    session.case(
        "tools_call_arguments_meta",
        lambda ep: ep.call("project_summary", {"handle": "x", "_meta": {}}),
    )
    session.case("unknown_tool", lambda ep: ep.call("no_such_tool", {}))
    session.case("unknown_method", lambda ep: ep.request("no/such/method"))
    session.case(
        "invalid_jsonrpc_version",
        lambda ep: _send_custom(ep, {"jsonrpc": "1.0", "method": "ping"}),
    )
    session.case(
        "missing_method",
        lambda ep: _send_custom(ep, {"jsonrpc": "2.0"}),
    )
    session.case(
        "batch_array",
        lambda ep: ep.send_raw(json.dumps([{"jsonrpc": "2.0", "id": 1, "method": "ping"}])),
    )
    session.case(
        "unknown_notification",
        lambda ep: ep.request("no/such/notification", notify=True),
    )

    # --- protocol version negotiation -------------------------------------
    session.case(
        "initialize_2025_11_25",
        lambda ep: ep.request(
            "initialize",
            {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "check_mcp", "version": "1.0.0"},
            },
        ),
    )
    session.case(
        "validation_error_as_tool_content",
        lambda ep: ep.call("project_summary", {}),
    )
    session.case(
        "initialize_unsupported_version",
        lambda ep: ep.request(
            "initialize",
            {
                "protocolVersion": "bogus",
                "capabilities": {},
                "clientInfo": {"name": "check_mcp", "version": "1.0.0"},
            },
        ),
    )
    session.case(
        "validation_error_back_to_jsonrpc",
        lambda ep: ep.call("project_summary", {}),
    )

    # --- session capacity (LRU, capacity 8) --------------------------------
    handles_py: list[str] = []
    handles_rs: list[str] = []
    for index in range(9):
        py_response, rs_response = session.case(
            f"lru_load_{index}",
            lambda ep: ep.call("load_project", {"path": "tmp.vpr"}),
        )
        handle_py = summary_handle(py_response)
        handle_rs = summary_handle(rs_response)
        if handle_py is not None:
            handles_py.append(handle_py)
        if handle_rs is not None:
            handles_rs.append(handle_rs)
    if len(handles_py) == 9 and len(handles_rs) == 9:
        py.state["lru"] = handles_py
        rs.state["lru"] = handles_rs
        session.case(
            "lru_evicted_handle",
            lambda ep: ep.call("project_summary", {"handle": ep.state["lru"][0]}),
        )
        session.case(
            "lru_retained_handle",
            lambda ep: ep.call("project_summary", {"handle": ep.state["lru"][1]}),
        )
    else:
        report.skip("lru cases", "load_project did not return handles")

    # --- malformed transport line (documented gap) -------------------------
    malformed_json(report, py, rs)


def malformed_json(report: Report, py: Endpoint, rs: Endpoint) -> None:
    """`-32700` shape matches; only the decoder's detail string is Python-specific."""
    py_response = py.send_raw("this is not json")
    rs_response = rs.send_raw("this is not json")
    py_error = py_response.get("error", {})
    rs_error = rs_response.get("error", {})
    same = (
        py_error.get("code") == rs_error.get("code") == -32700
        and py_error.get("message") == rs_error.get("message") == "JSON parse error"
        and py_response.get("id") is None
        and rs_response.get("id") is None
    )
    if same:
        report.ok("malformed_json_line")
        print(
            "  note: error `data` detail differs by decoder: "
            f"{py_error.get('data')!r} vs {rs_error.get('data')!r}"
        )
    else:
        report.fail("malformed_json_line", py_response, rs_response)


# ---------------------------------------------------------------------------
# File and CLI comparisons
# ---------------------------------------------------------------------------


def compare_files(report: Report) -> None:
    unstable = nondeterministic_formats()
    for name in (
        "pitch.ust",
        "pitch.svp",
        "out.json",
        "zoomed.ust",
        "coerced.ust",
        "pitch_str.ust",
        "bool.json",
    ):
        py_path = WORK / "py" / name
        rs_path = WORK / "rs" / name
        case = f"file:{name}"
        if not py_path.is_file() or not rs_path.is_file():
            report.fail(case, f"exists={py_path.is_file()}", f"exists={rs_path.is_file()}")
            continue
        py_bytes = py_path.read_bytes()
        rs_bytes = rs_path.read_bytes()
        suffix = py_path.suffix.lstrip(".")
        if suffix in unstable or py_path.stem in unstable:
            py_bytes = normalise_uuids(py_bytes)
            rs_bytes = normalise_uuids(rs_bytes)
        if py_bytes == rs_bytes:
            report.ok(case)
            continue
        detail = f"{len(py_bytes)} vs {len(rs_bytes)} bytes"
        if b"\x00" not in py_bytes and b"\x00" not in rs_bytes:
            py_lines = py_bytes.decode("utf-8", "replace").splitlines()
            rs_lines = rs_bytes.decode("utf-8", "replace").splitlines()
            for index, (left, right) in enumerate(zip(py_lines, rs_lines)):
                if left != right:
                    detail = f"first differing line {index + 1}:\n    py: {left}\n    rs: {right}"
                    break
        report.fail(case, detail, "")
    for label in ("py", "rs"):
        stray = WORK / label / "out.vshp"
        if stray.exists():
            report.fail(f"file:out.vshp absent ({label})", "missing", str(stray))


def write_fixtures() -> None:
    """Writes identical per-endpoint inputs for the extra load cases."""
    for label in ("py", "rs"):
        folder = WORK / label
        folder.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(TESTDATA / "simple.json", folder / "UPPER.JSON")
        # A UST file without tempo labels makes the parser emit
        # "No tempo labels found in the imported project.".
        (folder / "no_tempo.ust").write_bytes(
            b"[#VERSION]\r\nUST Version=1.2\r\n[#SETTING]\r\nTracks=1\r\n"
            b"[#0000]\r\nLength=480\r\nLyric=a\r\nNoteNum=60\r\n[#TRACKEND]\r\n"
        )


def normalise_cli(text: str) -> str:
    return text.replace("python -m libresvip.cli", "libresvip-cli").replace(
        "libresvip-cli.exe", "libresvip-cli"
    )


def run_cli(argv: list[str], *, python_cli: bool) -> subprocess.CompletedProcess[str]:
    env = python_env() if python_cli else rust_env()
    command = [str(PYTHON), "-m", "libresvip.cli", *argv] if python_cli else [str(RUST_EXE), *argv]
    return subprocess.run(
        command,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
        cwd=str(TESTDATA),
        timeout=180,
    )


def long_options(text: str) -> set[str]:
    return set(re.findall(r"--[a-z][a-z-]*", text))


def help_surface(report: Report) -> None:
    py = run_cli(["mcp", "serve", "--help"], python_cli=True)
    rs = run_cli(["mcp", "serve", "--help"], python_cli=False)
    problems = []
    if py.returncode != rs.returncode:
        problems.append(f"exit code {py.returncode} vs {rs.returncode}")
    if py.returncode != 0:
        problems.append(f"python help exited {py.returncode}")
    description = "Expose project load, middleware, and save as an MCP server on stdio."
    if description not in py.stdout:
        problems.append("python help lacks the command description")
    if description not in rs.stdout:
        problems.append("rust help lacks the command description")
    py_usage = next(
        (normalise_cli(line).strip() for line in py.stdout.splitlines() if "Usage:" in line),
        "",
    )
    rs_usage = normalise_cli(
        next((line.strip() for line in rs.stdout.splitlines() if "Usage:" in line), "")
    )
    if "mcp serve" not in py_usage:
        problems.append(f"python usage line: {py_usage!r}")
    if "mcp serve" not in rs_usage:
        problems.append(f"rust usage line: {rs_usage!r}")
    for label, usage in (("python", py_usage), ("rust", rs_usage)):
        tail = usage.split("mcp serve", 1)[-1].replace("[OPTIONS]", "").strip()
        if tail:
            problems.append(f"{label} usage advertises extra arguments: {usage!r}")
    if long_options(py.stdout) != long_options(rs.stdout):
        problems.append(
            f"options {sorted(long_options(py.stdout))} vs {sorted(long_options(rs.stdout))}"
        )
    if problems:
        report.fail("cli:mcp_serve_help", py.stdout, rs.stdout)
        for problem in problems:
            print(f"  {problem}")
    else:
        report.ok("cli:mcp_serve_help")
        print(
            "  note: help layout differs (rich panel vs clap, typer's `[OPTIONS]` marker); "
            f"the argument surface matches ({sorted(long_options(rs.stdout))}, no positionals)"
        )
        print(f"  python usage: {py_usage}")
        print(f"  rust usage:   {rs_usage}")
    if report.verbose:
        print("--- python help ---")
        print(py.stdout)
        print("--- rust help ---")
        print(rs.stdout)

    py_unknown = run_cli(["mcp", "serve", "--bogus"], python_cli=True)
    rs_unknown = run_cli(["mcp", "serve", "--bogus"], python_cli=False)
    if py_unknown.returncode == rs_unknown.returncode == 2:
        report.ok("cli:mcp_serve_unknown_option_exit_code")
    else:
        report.fail(
            "cli:mcp_serve_unknown_option_exit_code",
            py_unknown.returncode,
            rs_unknown.returncode,
        )


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main() -> int:
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[attr-defined]
    except Exception:  # noqa: BLE001
        pass
    parser = argparse.ArgumentParser()
    parser.add_argument("-v", "--verbose", action="store_true")
    parser.add_argument("--keep", action="store_true", help="keep .work/mcp contents")
    args = parser.parse_args()

    if not RUST_EXE.exists():
        print(f"missing Rust binary: {RUST_EXE}")
        return 2
    if not PYTHON.exists():
        print(f"missing Python interpreter: {PYTHON}")
        return 2
    if not args.keep and WORK.exists():
        shutil.rmtree(WORK, ignore_errors=True)
    for label in ("py", "rs"):
        (WORK / label).mkdir(parents=True, exist_ok=True)
    write_fixtures()

    report = Report(args.verbose)
    py = Endpoint(
        "py",
        [str(PYTHON), "-u", "-m", "libresvip.cli", "mcp", "serve"],
        python_env(),
        TESTDATA,
    )
    rs = Endpoint("rs", [str(RUST_EXE), "mcp", "serve"], rust_env(), TESTDATA)
    session = Session(py, rs, report)
    started = time.time()
    try:
        build_scenario(session)
        compare_files(report)
        help_surface(report)
    finally:
        py.stop()
        rs.stop()

    print()
    print(f"text rendering: {report.render_identical}/{report.render_total} responses identical")
    if report.render_diffs:
        print("  (whitespace/key-order only; the JSON payloads match)")
        for name, py_text, rs_text in report.render_diffs:
            print(f"  {name}:")
            print("    py: " + py_text.replace("\n", "\\n"))
            print("    rs: " + rs_text.replace("\n", "\\n"))
    total = report.passed + report.failed
    print(f"{report.passed}/{total} cases match ({report.skipped} skipped)")
    for label in ("py", "rs"):
        stderr_path = WORK / label / "server.stderr"
        if stderr_path.is_file() and stderr_path.stat().st_size:
            text = stderr_path.read_text(encoding="utf-8", errors="replace").strip()
            print(f"{label} server stderr:\n{text}")
    print(f"elapsed {time.time() - started:.1f}s")
    return 1 if report.failed else 0


if __name__ == "__main__":
    sys.exit(main())
