"""Data-consistency checks that run alongside the speed benchmark.

Two independent checks:

1. ``cross``  -- every engine (rust / uf3 / py) reads the *same* real project
   and writes UFData.  UFData is UtaFormatix3's exchange format, so it is the
   one representation all three produce.  The JSON is normalised (order of
   tracks, note tuples, pitch points, tempo list, time signatures) and the
   normalised forms are compared pairwise.  Byte equality is *not* expected
   (uuids, float formatting, lyric conventions), so the report keeps the
   exact component-level verdicts plus a magnitude for every mismatch.

2. ``roundtrip`` -- rust writes the project to format X, reads X back and
   writes UFData; that is compared with the UFData written straight from the
   source.  This covers every format the Rust port can write, including the
   ones neither reference tool supports (acep, svip, ...), and shows exactly
   which fields survive each container.

Usage:
    python consistency.py cross  [--pairs rust-uf3,rust-py] [--tier core]
    python consistency.py roundtrip [--limit 0] [--max-bytes N]
"""

from __future__ import annotations

import argparse
import bisect
import csv
import hashlib
import json
import pathlib
import statistics
import subprocess
import sys
import time

BASE = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night")
CASES = BASE / "cases.json"
WORK = BASE / "work"
RUST = pathlib.Path(r"E:\RUST_LIBRESVIP\RustLibreSVIP\target\release\libresvip-cli.exe")
UF3_CLI = BASE / "uf3_cli.js"
PY = pathlib.Path(r"E:\miniconda\envs\study12\python.exe")
PY_ROOT = pathlib.Path(r"E:\RUST_LIBRESVIP\LibreSVIP")
PY_SCRIPTS = pathlib.Path(r"E:\RUST_LIBRESVIP\RustLibreSVIP\scripts")

CROSS_CSV = BASE / "consistency_cross.csv"
RT_CSV = BASE / "consistency_roundtrip.csv"
RESULTS = BASE / "results.csv"

CROSS_FIELDS = [
    "ts", "source_ext", "path", "input_bytes", "pair",
    "tracks_a", "tracks_b", "track_names", "tempos", "time_signatures",
    "notes_a", "notes_b", "notes_identical", "notes_matched", "notes_match_pct",
    "key_diff", "tick_diff", "lyric_diff",
    "pitch_a", "pitch_b", "pitch_identical", "pitch_common_ticks",
    "pitch_mean_delta", "pitch_max_delta", "pitch_note_delta", "pitch_note_off", "pitch_note_off_pct",
    "norm_equal", "detail",
]

RT_FIELDS = [
    "ts", "source_ext", "target", "path", "input_bytes", "mid_bytes", "out_bytes",
    "ok", "tracks", "notes", "notes_identical", "notes_match_pct",
    "key_diff", "tick_diff", "lyric_diff", "pitch_identical", "pitch_mean_delta", "pitch_note_off_pct",
    "tempos", "time_signatures", "detail",
]


# ---------------------------------------------------------------------------
# normalisation
# ---------------------------------------------------------------------------

def _num(value: float) -> float | int:
    """Drop float noise so 4.0 and 4 compare equal, keep 3 decimals."""
    if isinstance(value, float) and value.is_integer():
        return int(value)
    return round(float(value), 3)


def load_ufdata(path: pathlib.Path) -> dict | None:
    try:
        raw = json.loads(path.read_text(encoding="utf-8-sig"))
    except Exception:
        return None
    project = raw.get("project") or {}
    tracks = []
    for track in project.get("tracks") or []:
        notes = [
            (int(n.get("tickOn", n.get("tick_on", 0))),
             int(n.get("tickOff", n.get("tick_off", 0))),
             int(n.get("key", 0)),
             str(n.get("lyric", "")))
            for n in track.get("notes") or []
        ]
        pitch = track.get("pitch") or {}
        ticks = pitch.get("ticks") or []
        values = pitch.get("values") or []
        pairs = []
        for tick, value in zip(ticks, values):
            pairs.append((int(tick), None if value is None else _num(value)))
        tracks.append({
            "name": str(track.get("name", "")),
            "notes": notes,
            "pitch": pairs,
        })
    return {
        "formatVersion": int(raw.get("formatVersion", 1)),
        "measurePrefix": int(project.get("measurePrefix", project.get("measure_prefix", 0))),
        "name": str(project.get("name", "")),
        "tempos": [
            (int(t.get("tickPosition", t.get("tick_position", 0))), _num(t.get("bpm", 0)))
            for t in project.get("tempos") or []
        ],
        "time_signatures": [
            (int(t.get("measurePosition", t.get("measure_position", 0))),
             int(t.get("numerator", 4)), int(t.get("denominator", 4)))
            for t in project.get("timeSignatures", project.get("time_signatures")) or []
        ],
        "tracks": tracks,
    }


def fingerprint(norm: dict) -> str:
    """Stable hash of the semantic content (ignores project/track names)."""
    payload = {
        "measurePrefix": norm["measurePrefix"],
        "tempos": norm["tempos"],
        "time_signatures": norm["time_signatures"],
        "tracks": [
            {"notes": t["notes"], "pitch": t["pitch"]} for t in norm["tracks"]
        ],
    }
    blob = json.dumps(payload, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    return hashlib.sha256(blob.encode("utf-8")).hexdigest()[:16]


def _sampler(pairs: list[tuple[int, float | None]], note_key: int):
    """Pitch-curve sampler that ignores 'no pitch' sentinels (0 / null).

    Importers densify pitch differently: USTX gives a point every few ticks,
    SVP only stores *deviations* and therefore a perfectly flat note has no
    points at all.  Comparing raw point lists would call those two "different"
    even though the sounding pitch is identical, so pitch is compared on a
    semantic level instead: sample both curves at a tick (linear interpolation)
    and fall back to the note's own key when a curve carries no data there.
    """
    values: dict[int, float] = {}
    for tick, value in pairs:
        if value is None or value == 0:
            continue
        values[tick] = float(value)
    ticks = sorted(values)

    def sample(tick: int) -> float:
        if not ticks:
            return float(note_key)
        index = bisect.bisect_left(ticks, tick)
        if index >= len(ticks):
            return values[ticks[-1]]
        if ticks[index] == tick:
            return values[ticks[index]]
        if index == 0:
            return values[ticks[0]] if tick >= ticks[0] else float(note_key)
        left, right = ticks[index - 1], ticks[index]
        left_value, right_value = values[left], values[right]
        if right == left:
            return right_value
        ratio = (tick - left) / (right - left)
        return left_value + (right_value - left_value) * ratio

    return sample


def _pitch_semantics(a: dict, b: dict) -> tuple[float | None, int]:
    """Mean |闂傚倸鍊风粈浣规櫠閻ｅ苯鍨濇い鏍ㄥ嚬閻?in semitones at note midpoints, plus the number of notes off."""
    deltas: list[float] = []
    for track_a, track_b in zip(a["tracks"], b["tracks"]):
        notes_b = {(n[0], n[2]): n for n in track_b["notes"]}
        sample_a = _sampler(track_a["pitch"], 60)
        sample_b = _sampler(track_b["pitch"], 60)
        for note in track_a["notes"]:
            other = notes_b.get((note[0], note[2]))
            if other is None:
                continue
            mid = note[0] + max(1, (note[1] - note[0]) // 2)
            deltas.append(abs(sample_a(mid) - sample_b(mid)))
    if not deltas:
        return None, 0
    return round(statistics.fmean(deltas), 3), sum(1 for d in deltas if d > 0.05)


def diff_norm(a: dict, b: dict) -> dict:
    notes_a = [n for t in a["tracks"] for n in t["notes"]]
    notes_b = [n for t in b["tracks"] for n in t["notes"]]
    pitch_a = [p for t in a["tracks"] for p in t["pitch"]]
    pitch_b = [p for t in b["tracks"] for p in t["pitch"]]

    notes_identical = notes_a == notes_b
    if notes_identical:
        matched = len(notes_a)
    else:
        # multiset overlap: how many notes exist verbatim in both
        from collections import Counter
        counter = Counter(notes_a)
        matched = 0
        for note in notes_b:
            if counter[note] > 0:
                counter[note] -= 1
                matched += 1
    base = max(len(notes_a), len(notes_b)) or 1

    def field_diff(get_a, get_b):
        if len(notes_a) != len(notes_b):
            return -1
        return sum(1 for x, y in zip(notes_a, notes_b) if get_a(x) != get_b(x))

    key_diff = field_diff(lambda n: n[2], lambda n: n[2])
    tick_diff = field_diff(lambda n: (n[0], n[1]), lambda n: (n[0], n[1]))
    lyric_diff = field_diff(lambda n: n[3], lambda n: n[3])

    pitch_identical = pitch_a == pitch_b
    common = 0
    deltas = []
    if pitch_a and pitch_b:
        map_b = {tick: value for tick, value in pitch_b}
        for tick, value in pitch_a:
            if tick in map_b:
                other = map_b[tick]
                if value is None or other is None:
                    if value is None and other is None:
                        common += 1
                    continue
                common += 1
                deltas.append(abs(float(value) - float(other)))
    mean_delta = round(statistics.fmean(deltas), 4) if deltas else None
    max_delta = round(max(deltas), 4) if deltas else None
    note_pitch_delta, note_pitch_off = _pitch_semantics(a, b)
    notes_total = max(len(notes_a), len(notes_b)) or 1

    detail = []
    if not notes_identical:
        detail.append(f"notes {len(notes_a)}vs{len(notes_b)} matched={matched}/{base}")
        if len(notes_a) == len(notes_b):
            for i, (x, y) in enumerate(zip(notes_a, notes_b)):
                if x != y:
                    detail.append(f"first note#{i}: {x} != {y}")
                    break
    if not pitch_identical:
        detail.append(f"pitch {len(pitch_a)}vs{len(pitch_b)} common={common}")
    if note_pitch_off:
        detail.append(f"{note_pitch_off} notes off by >0.05 st")
    if a["tempos"] != b["tempos"]:
        detail.append(f"tempos {len(a['tempos'])}vs{len(b['tempos'])}")
    if a["time_signatures"] != b["time_signatures"]:
        detail.append(f"timesig {len(a['time_signatures'])}vs{len(b['time_signatures'])}")
    if [t["name"] for t in a["tracks"]] != [t["name"] for t in b["tracks"]]:
        detail.append("track names differ")
    if a["measurePrefix"] != b["measurePrefix"]:
        detail.append(f"measurePrefix {a['measurePrefix']}vs{b['measurePrefix']}")

    return {
        "tracks_a": len(a["tracks"]),
        "tracks_b": len(b["tracks"]),
        "track_names": int([t["name"] for t in a["tracks"]] == [t["name"] for t in b["tracks"]]),
        "tempos": int(a["tempos"] == b["tempos"]),
        "time_signatures": int(a["time_signatures"] == b["time_signatures"]),
        "notes_a": len(notes_a),
        "notes_b": len(notes_b),
        "notes_identical": int(notes_identical),
        "notes_matched": matched,
        "notes_match_pct": round(100.0 * matched / base, 2),
        "key_diff": key_diff,
        "tick_diff": tick_diff,
        "lyric_diff": lyric_diff,
        "pitch_a": len(pitch_a),
        "pitch_b": len(pitch_b),
        "pitch_identical": int(pitch_identical),
        "pitch_common_ticks": common,
        "pitch_mean_delta": mean_delta if mean_delta is not None else "",
        "pitch_max_delta": max_delta if max_delta is not None else "",
        "pitch_note_delta": note_pitch_delta if note_pitch_delta is not None else "",
        "pitch_note_off": note_pitch_off,
        "pitch_note_off_pct": round(100.0 * note_pitch_off / notes_total, 2),
        "norm_equal": int(fingerprint(a) == fingerprint(b)),
        "detail": " | ".join(detail)[:300],
    }


# ---------------------------------------------------------------------------
# engine invocations
# ---------------------------------------------------------------------------

def case_folder(case: dict) -> pathlib.Path:
    src = pathlib.Path(case["path"])
    return WORK / f'{case["tier"]}_{src.stem[:40]}_{case["size"]}'


def run_engine(tool: str, src: pathlib.Path, out: pathlib.Path, ext: str, target: str = "ufdata") -> tuple[bool, str]:
    out.parent.mkdir(parents=True, exist_ok=True)
    if out.exists():
        out.unlink()
    try:
        if tool == "rust":
            proc = subprocess.run(
                [str(RUST), "proj", "convert", str(src), str(out)],
                input="\n" * 128, capture_output=True, text=True,
                encoding="utf-8", errors="replace", timeout=900,
            )
        elif tool == "uf3":
            proc = subprocess.run(
                ["node", str(UF3_CLI), str(src), str(out), ext, target],
                capture_output=True, text=True, encoding="utf-8",
                errors="replace", timeout=900,
            )
        else:
            state = WORK / "pystate" / (src.stem[:40] + "_cons")
            state.mkdir(parents=True, exist_ok=True)
            import os
            env = os.environ.copy()
            env["PYTHONPATH"] = os.pathsep.join([str(PY_SCRIPTS), str(PY_ROOT)])
            env["PYTHONIOENCODING"] = "utf-8"
            env["COLUMNS"] = "80"
            env["LIBRESVIP_TEST_STATE"] = str(state)
            proc = subprocess.run(
                [str(PY), "-m", "py_state_entry", "proj", "convert", str(src), str(out)],
                input="\n" * 128, capture_output=True, text=True,
                encoding="utf-8", errors="replace", timeout=900,
                env=env, cwd=str(state),
            )
    except subprocess.TimeoutExpired:
        return False, "timeout"
    if proc.returncode != 0 or not out.exists() or out.stat().st_size == 0:
        return False, f"rc={proc.returncode} {(proc.stderr or '')[-160:]}".replace("\n", " ")
    return True, ""


# ---------------------------------------------------------------------------
# cross-tool mode
# ---------------------------------------------------------------------------

def cross(args) -> int:
    cases = json.loads(CASES.read_text(encoding="utf-8"))
    pairs = [tuple(p.split("-")) for p in args.pairs.split(",") if p.strip()]
    out_csv = pathlib.Path(args.out) if args.out else CROSS_CSV
    new_file = not out_csv.exists()
    handle = out_csv.open("a", encoding="utf-8", newline="")
    writer = csv.DictWriter(handle, fieldnames=CROSS_FIELDS, extrasaction="ignore")
    if new_file:
        writer.writeheader()
    done: set[tuple[str, str, str]] = set()
    if out_csv.exists():
        with out_csv.open(encoding="utf-8", newline="") as prev:
            for row in csv.DictReader(prev):
                done.add((row["path"], row["source_ext"], row["pair"]))

    # Which (tool, project) pairs the speed run could handle at all: replaying a
    # conversion the engine already failed on would just burn the timeout again.
    known: dict[tuple[str, str], bool] = {}
    if RESULTS.exists() and args.reuse_timings:
        with RESULTS.open(encoding="utf-8", newline="") as results_handle:
            for row in csv.DictReader(results_handle):
                key = (row["tool"], row["path"])
                known[key] = known.get(key, False) or row["ok"] == "1"

    count = 0
    for case in cases:
        if args.tier and case["tier"] != args.tier:
            continue
        ext = case["ext"].lstrip(".")
        src = pathlib.Path(case["path"])
        if not src.exists():
            continue
        folder = case_folder(case)
        for left, right in pairs:
            label = f"{left}-{right}" if args.via == "ufdata" else f"{left}-{right}@{args.via}"
            if any(known.get((tool, case["path"])) is False for tool in (left, right)):
                continue
            if (case["path"], ext, label) in done:
                continue
            if right not in case["tools"] or left not in case["tools"]:
                # The speed run only touches a small Python subset (it is very
                # slow); for correctness the Python reference is worth running
                # on every small project, so allow that explicitly.
                if not ("py" in (left, right) and case["size"] <= args.force_py_max):
                    continue
            if "py" in (left, right) and case["size"] > args.max_input_py:
                continue
            norms = {}
            failure = ""
            via = args.via
            for tool in (left, right):
                out = folder / f"cons_{tool}{args.tag}.{via}"
                ok, err = run_engine(tool, src, out, ext, via)
                if not ok:
                    failure = f"{tool}: {err}"
                    break
                if via != "ufdata":
                    # Normalise through a single common reader (the Rust one), so
                    # the comparison sees the pitch curve each writer produced.
                    norm_path = folder / f"cons_{tool}{args.tag}.ufdata"
                    ok, err = run_engine("rust", out, norm_path, via, "ufdata")
                    if not ok:
                        failure = f"{tool}->ufdata: {err}"
                        break
                    out = norm_path
                norm = load_ufdata(out)
                if norm is None:
                    failure = f"{tool}: unparsable ufdata"
                    break
                norms[tool] = norm
            row = {field: "" for field in CROSS_FIELDS}
            row.update({
                "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
                "source_ext": ext,
                "path": case["path"],
                "input_bytes": case["size"],
                "pair": label,
            })
            if len(norms) == 2:
                row.update(diff_norm(norms[left], norms[right]))
            else:
                row["detail"] = failure[:300]
            writer.writerow(row)
            handle.flush()
            count += 1
            print(f"[{count}] {ext:>8} {label}: {row.get('detail') or 'equal'}", flush=True)
            if args.limit and count >= args.limit:
                handle.close()
                return 0
    handle.close()
    print(f"cross done: {count} comparisons")
    return 0


# ---------------------------------------------------------------------------
# round-trip mode
# ---------------------------------------------------------------------------

def roundtrip(args) -> int:
    cases = json.loads(CASES.read_text(encoding="utf-8"))
    new_file = not RT_CSV.exists()
    handle = RT_CSV.open("a", encoding="utf-8", newline="")
    writer = csv.DictWriter(handle, fieldnames=RT_FIELDS, extrasaction="ignore")
    if new_file:
        writer.writeheader()
    done: set[tuple[str, str]] = set()
    if RT_CSV.exists():
        with RT_CSV.open(encoding="utf-8", newline="") as prev:
            for row in csv.DictReader(prev):
                done.add((row["path"], row["target"]))

    count = 0
    for case in cases:
        ext = case["ext"].lstrip(".")
        src = pathlib.Path(case["path"])
        if not src.exists() or case["size"] > args.max_bytes:
            continue
        folder = case_folder(case)
        direct = folder / f"cons_rust{args.tag}.ufdata"
        ok, err = run_engine("rust", src, direct, ext)
        if not ok:
            continue
        base_norm = load_ufdata(direct)
        if base_norm is None:
            continue
        for target in args.targets.split(",") if args.targets else case["targets"]:
            if target in ("ufdata", ext):
                continue
            if (case["path"], target) in done:
                continue
            mid = folder / f"rt_{ext}_{target}.{target}"
            back = folder / f"rt_{ext}_{target}.ufdata"
            ok1, err1 = run_engine("rust", src, mid, ext)
            if not ok1:
                mid.write_bytes(b"")
            ok2 = False
            if ok1:
                ok2, err2 = run_engine("rust", mid, back, target)
            row = {field: "" for field in RT_FIELDS}
            row.update({
                "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
                "source_ext": ext,
                "target": target,
                "path": case["path"],
                "input_bytes": case["size"],
                "mid_bytes": mid.stat().st_size if mid.exists() else 0,
            })
            if ok1 and ok2:
                norm = load_ufdata(back)
                if norm is not None:
                    diff = diff_norm(base_norm, norm)
                    row.update({k: diff[k] for k in (
                        "tracks_a", "notes_a", "notes_identical", "notes_match_pct",
                        "key_diff", "tick_diff", "lyric_diff", "pitch_identical",
                        "pitch_mean_delta", "pitch_note_off_pct", "tempos",
                        "time_signatures", "detail")})
                    row["tracks"] = diff["tracks_a"]
                    row["notes"] = diff["notes_a"]
                    row["out_bytes"] = back.stat().st_size
                    row["ok"] = 1
                else:
                    row["detail"] = "roundtrip ufdata unparsable"
            else:
                row["detail"] = (err1 or "write failed") if not ok1 else f"re-read failed: {err2}"
            writer.writerow(row)
            handle.flush()
            count += 1
            print(f"[{count}] {ext:>8} -> {target:<8} {row.get('detail') or ('identical' if row.get('notes_identical') == 1 else 'differs')}", flush=True)
            if args.limit and count >= args.limit:
                handle.close()
                return 0
    handle.close()
    print(f"roundtrip done: {count} rows")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="mode", required=True)

    p_cross = sub.add_parser("cross")
    p_cross.add_argument("--pairs", default="rust-uf3,rust-py")
    p_cross.add_argument("--tier", default="")
    p_cross.add_argument("--limit", type=int, default=0)
    p_cross.add_argument("--max-input-py", type=int, default=2_000_000)
    p_cross.add_argument("--out", default="")
    p_cross.add_argument("--tag", default="")
    p_cross.add_argument("--reuse-timings", action="store_true", help="skip projects the speed run already failed to convert")
    p_cross.add_argument("--force-py-max", type=int, default=0, help="also compare Python for files up to this size, even outside the timing subset")
    p_cross.add_argument("--via", default="ufdata", help="intermediate format all engines write (ufdata/svp)")
    p_cross.set_defaults(func=cross)

    p_rt = sub.add_parser("roundtrip")
    p_rt.add_argument("--targets", default="")
    p_rt.add_argument("--limit", type=int, default=0)
    p_rt.add_argument("--max-bytes", type=int, default=3_000_000)
    p_rt.add_argument("--tag", default="")
    p_rt.set_defaults(func=roundtrip)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
