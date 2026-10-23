"""Generate differential test vectors for `libresvip-core` from Python.

Writes `testdata/core_cases.json`, consumed by
`crates/libresvip-core/tests/core_cases.rs`.

Covers tick maths, `TimeSynchronizer`, `ParamCurve` sampling/segmentation and the
`reset_time_axis` / `zoom_project` / `limit_bars` helpers.

Usage: E:\\miniconda\\envs\\study12\\python.exe scripts/gen_core_cases.py
"""

from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
PY_ROOT = ROOT / "LibreSVIP"
if str(PY_ROOT) not in sys.path:
    sys.path.insert(0, str(PY_ROOT))

from libresvip.core.tick_counter import (  # noqa: E402
    find_bar_index,
    shift_beat_list,
    shift_tempo_list,
    skip_beat_list,
    skip_tempo_list,
)
from libresvip.core.time_sync import TimeSynchronizer  # noqa: E402
from libresvip.model.base import ParamCurve, Points, Project, SongTempo, TimeSignature  # noqa: E402
from libresvip.model.point import Point  # noqa: E402
from libresvip.model.reset_time_axis import limit_bars, reset_time_axis, zoom_project  # noqa: E402

TESTDATA = pathlib.Path(__file__).resolve().parent.parent / "testdata"

TEMPOS = [
    [SongTempo(position=0, bpm=120.0)],
    [SongTempo(position=0, bpm=120.0), SongTempo(position=1920, bpm=90.0)],
    [
        SongTempo(position=0, bpm=100.0),
        SongTempo(position=960, bpm=140.0),
        SongTempo(position=2880, bpm=75.5),
    ],
]

SIGNATURES = [
    [TimeSignature(bar_index=0, numerator=4, denominator=4)],
    [
        TimeSignature(bar_index=0, numerator=4, denominator=4),
        TimeSignature(bar_index=4, numerator=3, denominator=4),
    ],
    [
        TimeSignature(bar_index=0, numerator=3, denominator=8),
        TimeSignature(bar_index=2, numerator=6, denominator=8),
        TimeSignature(bar_index=5, numerator=4, denominator=4),
    ],
]

CURVES = [
    [[0, 0], [240, 10], [480, 0]],
    [[0, -100], [100, 0], [110, 5], [120, -100], [500, 3], [640, 7]],
    [[-192000, -100], [0, 0], [10, 1], [11, 2], [12, 3], [1073741823, -100]],
    [[0, 5]],
    [],
]


def curve_json(points: list[list[int]]) -> list[list[int]]:
    return points


def main() -> None:
    cases: dict[str, object] = {}

    tempo_cases = []
    for tempos in TEMPOS:
        for skip in (0, 240, 1920, 3000, 100000):
            tempo_cases.append(
                {
                    "tempos": [t.model_dump(by_alias=True) for t in tempos],
                    "skip": skip,
                    "expected": [t.model_dump(by_alias=True) for t in skip_tempo_list(tempos, skip)],
                    "shifted": [
                        t.model_dump(by_alias=True) for t in shift_tempo_list(tempos, 480)
                    ],
                }
            )
    cases["tempo"] = tempo_cases

    beat_cases = []
    for signatures in SIGNATURES:
        for skip in (0, 1, 4, 10):
            beat_cases.append(
                {
                    "signatures": [s.model_dump(by_alias=True) for s in signatures],
                    "skip": skip,
                    "expected": [
                        s.model_dump(by_alias=True) for s in skip_beat_list(signatures, skip)
                    ],
                    "shifted": [
                        s.model_dump(by_alias=True) for s in shift_beat_list(signatures, 2)
                    ],
                }
            )
    cases["beat"] = beat_cases

    bar_cases = []
    for signatures in SIGNATURES:
        for ticks in (0, 100, 480, 1919, 1920, 5000, 100000):
            bar_cases.append(
                {
                    "signatures": [s.model_dump(by_alias=True) for s in signatures],
                    "ticks": ticks,
                    "expected": find_bar_index(signatures, ticks),
                }
            )
    cases["bar"] = bar_cases

    sync_cases = []
    for tempos in TEMPOS:
        for skip in (0, 480):
            for absolute in (False, True):
                for default_tempo in (60.0, 120.0):
                    synchronizer = TimeSynchronizer(
                        tempos,
                        skip_ticks=skip,
                        _is_absolute_time_code=absolute,
                        _default_tempo=default_tempo,
                    )
                    query_ticks = [0, 100, 480, 960, 1920, 2880, 5000]
                    sync_cases.append(
                        {
                            "tempos": [t.model_dump(by_alias=True) for t in tempos],
                            "skip": skip,
                            "absolute": absolute,
                            "default_tempo": default_tempo,
                            "ticks": query_ticks,
                            "secs": [synchronizer.get_actual_secs_from_ticks(t) for t in query_ticks],
                            "actual_ticks": [
                                synchronizer.get_actual_ticks_from_ticks(t) for t in query_ticks
                            ],
                            "secs_offsets": [
                                synchronizer.get_actual_ticks_from_secs_offset(480, 0.5),
                                synchronizer.get_actual_ticks_from_secs_offset(0, 2.0),
                            ],
                            "batch": synchronizer.get_actual_secs_from_ticks_batch(query_ticks),
                        }
                    )
    cases["sync"] = sync_cases

    curve_cases = []
    for points in CURVES:
        for interval in (0, 5, 50):
            curve = ParamCurve(points=Points(root=[Point(*p) for p in points]))
            curve_cases.append(
                {
                    "points": curve_json(points),
                    "interval": interval,
                    "interrupt": 0,
                    "reduced": [list(p) for p in curve.reduce_sample_rate(interval).points.root],
                    "segments": [
                        [list(p) for p in segment]
                        for segment in curve.split_into_segments(0)
                    ],
                }
            )
    cases["curve"] = curve_cases

    simple = Project.model_validate_json((TESTDATA / "simple.json").read_bytes().decode("utf-8"))
    projects = {"simple": simple.model_dump(mode="json", by_alias=True)}
    dynamic = simple.model_copy(
        update={
            "song_tempo_list": [
                SongTempo(position=0, bpm=120.0),
                SongTempo(position=960, bpm=90.0),
            ]
        }
    )
    projects["dynamic"] = dynamic.model_dump(mode="json", by_alias=True)

    transform_cases = []
    for name, project in (("simple", simple), ("dynamic", dynamic)):
        transform_cases.append(
            {
                "name": name,
                "expected_zoom_2": zoom_project(project, 2.0).model_dump(mode="json", by_alias=True),
                "expected_zoom_half": zoom_project(project, 0.5).model_dump(
                    mode="json", by_alias=True
                ),
                "expected_limit_1": limit_bars(project, 1).model_dump(mode="json", by_alias=True),
                "expected_reset": reset_time_axis(project, 100.0).model_dump(
                    mode="json", by_alias=True
                ),
            }
        )
    cases["transforms"] = transform_cases
    cases["projects"] = projects

    out = TESTDATA / "core_cases.json"
    out.write_text(json.dumps(cases, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
