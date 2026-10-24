"""Generate the parity fixtures for the ported upstream experimental/deprecated
plugins under ``testdata/experimental/``.

Writes for each format:
* ``input.<suffix>`` — a synthetic project exercising the ported code paths;
* ``expected_project.json`` — the hub model Python parses it into;
* ``expected_dump.<suffix>`` — Python's re-dump of that model.

The msq fixtures were generated once by a throwaway script against the same
Python code; regenerate them the same way if ``experimental/msq`` changes.

Usage (from the workspace root):
    E:\\miniconda\\envs\\study12\\python.exe scripts/gen_experimental_fixtures.py
"""

from __future__ import annotations

import copy
import json
import pathlib
import shutil
import struct
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
PYTHON_LIBRESVIP = pathlib.Path(r"E:\RUST_LIBRESVIP\LibreSVIP")
OUT = ROOT / "testdata" / "experimental"

GJ_INPUT = {
    "gjgjVersion": 2,
    "TempoMap": {
        "TicksPerQuarterNote": 480,
        "Tempos": [
            {"Time": 0, "MicrosecondsPerQuarterNote": 500000},
            {"Time": 1920, "MicrosecondsPerQuarterNote": 400000},
        ],
        "TimeSignature": [{"Time": 0, "Numerator": 4, "Denominator": 4}],
    },
    "Tracks": [
        {
            "ID": "1",
            "Name": "513singer",
            "BeatItems": [
                {
                    "ID": 1,
                    "Lyric": "你",
                    "Pinyin": "ni",
                    "StartTick": 1920,
                    "Duration": 480,
                    "Track": 60,
                    "Style": 1,
                    "PreTime": 50.0,
                    "PostTime": -100.0,
                    "Velocity": 127,
                },
                {
                    "ID": 2,
                    "Lyric": " 好 ",
                    "Pinyin": "hao",
                    "StartTick": 2400,
                    "Duration": 480,
                    "Track": 62,
                    "Style": 2,
                    "PreTime": 0,
                    "PostTime": 0,
                },
            ],
            "Tone": {
                "Modifys": [{"X": 0, "Y": 100}, {"X": 10, "Y": 200}],
                "ModifyRanges": [{"X": 0, "Y": 10}],
            },
            "VolumeMap": [{"Time": 1920, "Volume": 0.5}],
            "SingerInfo": {"DisplayName": "513singer"},
        }
    ],
    "Accompaniments": [{"ID": "2", "Path": "acc.wav", "Offset": 10000000}],
}


def make_gj_fixture(staging: pathlib.Path) -> None:
    # Import the deprecated plugin as a package.
    package_root = staging / "gjpkg"
    (package_root / "gj").mkdir(parents=True)
    for name in (
        "model.py",
        "gjgj_parser.py",
        "gjgj_generator.py",
        "gjgj_converter.py",
        "options.py",
        "singers.py",
        "constants.py",
    ):
        shutil.copy(
            PYTHON_LIBRESVIP / "deprecated" / "gj" / name,
            package_root / "gj" / name,
        )
    (package_root / "gj" / "__init__.py").write_text("", encoding="utf-8")
    sys.path.insert(0, str(package_root))
    sys.path.insert(0, str(PYTHON_LIBRESVIP))

    from gj.model import GjgjProject  # noqa: E402
    from gj.gjgj_parser import GjgjParser  # noqa: E402
    from gj.gjgj_generator import GjgjGenerator  # noqa: E402
    from gj.options import InputOptions, OutputOptions  # noqa: E402

    target = OUT / "gj"
    target.mkdir(parents=True, exist_ok=True)
    input_bytes = (
        json.dumps(GJ_INPUT, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        + b"\xef\xbb\xbf"  # written after prefix swap below
    )
    # Python writes BOM-prefixed utf-8; keep the same order for the fixture.
    input_bytes = b"\xef\xbb\xbf" + json.dumps(
        GJ_INPUT, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")
    (target / "input.gj").write_bytes(input_bytes)

    gj = GjgjProject.model_validate(GJ_INPUT)
    parsed = GjgjParser(InputOptions()).parse_project(gj)
    (target / "expected_project.json").write_text(
        parsed.model_dump_json(by_alias=True, exclude_none=True), encoding="utf-8"
    )

    regenerated = GjgjGenerator(OutputOptions(down_sample=32, singer="扇宝")).generate_project(parsed)
    (target / "expected_dump.gj").write_bytes(
        b"\xef\xbb\xbf"
        + json.dumps(
            regenerated.model_dump(mode="json", exclude_none=True, by_alias=True),
            ensure_ascii=False,
            separators=(",", ":"),
        ).encode("utf-8")
    )
    print("gj fixture written")


def build_vsp(project_dict: object, project_file: object) -> bytes:
    """`vsp_converter.dump`: build + patch `file_size` at offset 8."""
    content = bytearray(project_file.build(project_dict))
    struct.pack_into("<I", content, 8, len(content))
    return bytes(content)


def make_vsp_fixture(staging: pathlib.Path) -> None:
    package_root = staging / "vsppkg"
    (package_root / "vsp").mkdir(parents=True)
    for name in (
        "model.py",
        "vsp_parser.py",
        "vsp_generator.py",
        "vsp_converter.py",
        "options.py",
        "constants.py",
    ):
        shutil.copy(
            PYTHON_LIBRESVIP / "experimental" / "vsp" / name,
            package_root / "vsp" / name,
        )
    (package_root / "vsp" / "__init__.py").write_text("", encoding="utf-8")
    sys.path.insert(0, str(package_root))
    sys.path.insert(0, str(PYTHON_LIBRESVIP))

    from vsp.model import VocalinaStudioProjectFile  # noqa: E402
    from vsp.options import InputOptions, OutputOptions  # noqa: E402
    from vsp.vsp_generator import VspGenerator  # noqa: E402
    from vsp.vsp_parser import VspParser  # noqa: E402

    from libresvip.model.base import (  # noqa: E402
        Note,
        ParamCurve,
        Params,
        Project,
        SingingTrack,
        SongTempo,
        TimeSignature,
    )

    project = Project(
        song_tempo_list=[
            SongTempo(position=0, bpm=120.0),
            SongTempo(position=1920, bpm=150.0),
        ],
        time_signature_list=[
            TimeSignature(bar_index=0, numerator=4, denominator=4),
            TimeSignature(bar_index=4, numerator=3, denominator=4),
        ],
        track_list=[
            SingingTrack(
                title="主旋律",
                ai_singer_name="test_singer",
                volume=0.8,
                pan=-0.2,
                note_list=[
                    Note(start_pos=960, length=480, key_number=60, lyric="你"),
                    Note(start_pos=1440, length=480, key_number=62, lyric="好"),
                    Note(start_pos=2400, length=960, key_number=64, lyric="呀"),
                ],
                edited_params=Params(
                    pitch=ParamCurve(
                        points=[
                            (0, -100),
                            (960, 100),
                            (1200, 200),
                            (1440, 0),
                            (2400, -100),
                            (2600, 300),
                            (3000, -100),
                            (3400, -100),
                        ]
                    )
                ),
            ),
            SingingTrack(
                title="和声",
                note_list=[
                    Note(start_pos=960, length=960, key_number=55, lyric="la"),
                ],
            ),
        ],
    )

    target = OUT / "vsp"
    target.mkdir(parents=True, exist_ok=True)

    generated = VspGenerator(OutputOptions()).generate_project(project)
    input_bytes = build_vsp(generated, VocalinaStudioProjectFile)
    (target / "input.vsp").write_bytes(input_bytes)

    parsed = VspParser(InputOptions(), target / "input.vsp").parse_project(
        VocalinaStudioProjectFile.parse(input_bytes)
    )
    (target / "expected_project.json").write_text(
        json.dumps(
            parsed.model_dump(mode="json", by_alias=True, exclude_none=True),
            ensure_ascii=False,
            separators=(",", ":"),
        ),
        encoding="utf-8",
    )

    regenerated = VspGenerator(OutputOptions()).generate_project(parsed)
    (target / "expected_dump.vsp").write_bytes(
        build_vsp(regenerated, VocalinaStudioProjectFile)
    )

    # Second fixture: the generator never emits BGM, so plant PCM into the
    # bgm section by hand and let the parser extract the wav next to the
    # project file.
    pcm = bytes((index % 255) + 1 for index in range(1764))
    bgm_dict = copy.deepcopy(regenerated)
    bgm_dict["bgm"]["data"]["raw_data"] = pcm
    bgm_bytes = build_vsp(bgm_dict, VocalinaStudioProjectFile)
    (target / "bgm_input.vsp").write_bytes(bgm_bytes)

    with tempfile.TemporaryDirectory() as bgm_dir:
        bgm_path = pathlib.Path(bgm_dir) / "input.vsp"
        bgm_path.write_bytes(bgm_bytes)
        bgm_parsed = VspParser(
            InputOptions(extract_audio=True), bgm_path
        ).parse_project(VocalinaStudioProjectFile.parse(bgm_bytes))
        instrumental = [
            track
            for track in bgm_parsed.track_list
            if type(track).__name__ == "InstrumentalTrack"
        ]
        assert len(instrumental) == 1, "expected the extracted bgm track"
        shutil.copy(bgm_path.with_suffix(".wav"), target / "bgm_expected.wav")

    print("vsp fixtures written")


def make_ppsf_legacy_fixture(staging: pathlib.Path) -> None:
    """Parity fixtures for the legacy binary PPSF container (a regular
    upstream plugin, not an isolated one — imported directly)."""
    from libresvip.model.base import (  # noqa: E402
        Note,
        ParamCurve,
        Params,
        Project,
        SingingTrack,
        SongTempo,
        TimeSignature,
    )
    from libresvip.plugins.ppsf.piapro_studio_legacy_generator import (  # noqa: E402
        PiaproStudioLegacyGenerator,
    )
    from libresvip.plugins.ppsf.piapro_studio_legacy_parser import (  # noqa: E402
        PiaproStudioLegacyParser,
    )
    from libresvip.plugins.ppsf.legacy_model import PpsfLegacyProject  # noqa: E402
    from libresvip.plugins.ppsf.options import InputOptions, OutputOptions  # noqa: E402

    project = Project(
        song_tempo_list=[
            SongTempo(position=0, bpm=120.0),
            SongTempo(position=1920, bpm=150.0),
        ],
        time_signature_list=[
            TimeSignature(bar_index=0, numerator=4, denominator=4),
            TimeSignature(bar_index=4, numerator=3, denominator=4),
        ],
        track_list=[
            SingingTrack(
                title="主旋律",
                note_list=[
                    Note(start_pos=960, length=480, key_number=60, lyric="你", pronunciation="ni3"),
                    Note(start_pos=1440, length=480, key_number=62, lyric="好", pronunciation="hao3"),
                    Note(start_pos=2400, length=960, key_number=64, lyric="呀", pronunciation="ya5"),
                ],
                edited_params=Params(
                    pitch=ParamCurve(
                        points=[
                            (0, -100),
                            (960, 100),
                            (1200, 200),
                            (1440, 0),
                            (2400, -100),
                            (2600, 300),
                            (3000, -100),
                            (3400, -100),
                        ]
                    )
                ),
            ),
            SingingTrack(
                title="和声",
                note_list=[
                    Note(start_pos=960, length=960, key_number=55, lyric="la", pronunciation="4 a"),
                ],
            ),
        ],
    )

    target = OUT / "ppsf_legacy"
    target.mkdir(parents=True, exist_ok=True)
    generator = PiaproStudioLegacyGenerator(OutputOptions())
    input_bytes = generator.generate_project(project)
    (target / "input.ppsf").write_bytes(input_bytes)

    parsed = PiaproStudioLegacyParser(InputOptions()).parse_project(
        PpsfLegacyProject.parse(input_bytes)
    )
    (target / "expected_project.json").write_text(
        json.dumps(
            parsed.model_dump(mode="json", by_alias=True, exclude_none=True),
            ensure_ascii=False,
            separators=(",", ":"),
        ),
        encoding="utf-8",
    )

    regenerated = generator.generate_project(parsed)
    (target / "expected_dump.ppsf").write_bytes(regenerated)
    print("ppsf legacy fixtures written")
def main() -> int:
    with tempfile.TemporaryDirectory() as raw:
        staging = pathlib.Path(raw)
        make_gj_fixture(staging)
        make_vsp_fixture(staging)
        make_ppsf_legacy_fixture(staging)
    return 0


if __name__ == "__main__":
    sys.exit(main())
