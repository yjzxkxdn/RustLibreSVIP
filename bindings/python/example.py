"""Convert one project file with the Python binding.

    python example.py song.ustx svp song.svp
    python example.py song.ustx svp song.svp ustx

The engine is found next to this file, in `target/release`, or through
`RUSTLIBRESVIP_LIB`.
"""

import sys

from rustlibresvip import RustLibreSVIP


def main(argv: list[str]) -> int:
    if len(argv) < 4:
        print("usage: example.py <input> <output-format> <output-file> [input-format]")
        return 2

    engine = RustLibreSVIP()
    formats = engine.formats()
    print(f"rustlibresvip {engine.version} - {len(formats)} formats")

    warnings = engine.convert_file(
        argv[1],
        argv[3],
        argv[2],
        argv[4] if len(argv) > 4 else None,
    )
    for warning in warnings:
        print(f"  . {warning['kind']}: {warning['detail']}")
    print(f"wrote {argv[3]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
