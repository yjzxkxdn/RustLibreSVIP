"""Generate a large benchmark project (`testdata/bench_large.json`).

10 tracks x 600 notes, plus a dense pitch curve per track — similar to a real
full-length multi-track session, large enough to expose algorithmic differences.

Usage: E:\\miniconda\\envs\\study12\\python.exe scripts/gen_bench.py
"""

from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
PY_ROOT = ROOT / "LibreSVIP"
if str(PY_ROOT) not in sys.path:
    sys.path.insert(0, str(PY_ROOT))

from libresvip.model.base import (  # noqa: E402
    InstrumentalTrack,
    Note,
    ParamCurve,
    Params,
    Points,
    Project,
    SingingTrack,
    SongTempo,
    TimeSignature,
)
from libresvip.model.point import Point  # noqa: E402

TESTDATA = pathlib.Path(__file__).resolve().parent.parent / "testdata"

TRACKS = 10
NOTES_PER_TRACK = 600
PITCH_POINTS = 4000


def build_track(index: int) -> SingingTrack:
    notes = []
    position = 0
    for note_index in range(NOTES_PER_TRACK):
        length = 120 + (note_index * 37) % 360
        notes.append(
            Note(
                start_pos=position,
                length=length,
                key_number=48 + (note_index * 7) % 36,
                lyric=["la", "li", "lu", "le", "lo"][note_index % 5],
            )
        )
        position += length + (note_index % 3) * 30
    pitch_points = [
        Point(i * 12, ((i * 37) % 2400) - 1200) for i in range(PITCH_POINTS)
    ]
    volume_points = [Point(i * 24, (i % 100) - 50) for i in range(PITCH_POINTS // 2)]
    return SingingTrack(
        title=f"Track {index + 1}",
        ai_singer_name="Singer",
        note_list=notes,
        edited_params=Params(
            pitch=ParamCurve(points=Points(root=pitch_points)),
            volume=ParamCurve(points=Points(root=volume_points)),
        ),
    )


def main() -> None:
    project = Project(
        version="1.0.0",
        song_tempo_list=[
            SongTempo(position=0, bpm=120.0),
            SongTempo(position=480 * 64, bpm=96.0),
        ],
        time_signature_list=[
            TimeSignature(bar_index=0, numerator=4, denominator=4),
            TimeSignature(bar_index=32, numerator=3, denominator=4),
        ],
        track_list=[build_track(i) for i in range(TRACKS)]
        + [InstrumentalTrack(title="Inst", audio_file_path="audio.wav")],
    )
    out = TESTDATA / "bench_large.json"
    out.write_bytes(
        json.dumps(project.model_dump(mode="json", by_alias=True), ensure_ascii=False).encode("utf-8")
    )
    notes = sum(len(t.note_list) for t in project.track_list if isinstance(t, SingingTrack))
    print(f"wrote {out} ({out.stat().st_size / 1024:.1f} KiB, {notes} notes)")


if __name__ == "__main__":
    main()
