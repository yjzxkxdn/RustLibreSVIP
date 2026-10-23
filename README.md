# RustLibreSVIP

Pure-Rust port of [LibreSVIP](https://github.com/SoulMelody/LibreSVIP) **2.9.2** —
the CLI only, no GUI. The Python implementation in `../LibreSVIP` is the source
of truth; every command, option schema and file format is ported to match it.

## Status

| area | state |
| --- | --- |
| `libresvip-core` models, serde aliases, tick math, time sync, settings, gettext | done |
| CLI commands `conf`, `plugin list/detail/toggle` | done, byte-identical output |
| CLI `proj convert/split/merge` (interactive prompts) | done |
| Middlewares (6) | ported (Chinese pinyin/romaji conversions still pending) |
| Format plugins (40) | **all 40 ported and golden-verified** |
| CLI parity harness | **90/90 cases** (1 documented SKIP where LibreSVIP 2.9.2 crashes) |
| `rpc server` differential harness | **45/45 cases** (`scripts/check_rpc.py`) |
| `mcp serve` differential harness | **87/87 cases** (`scripts/check_mcp.py`) |
| `rpc server` (gRPC) / `mcp serve` (MCP stdio) | done, differentially verified |

`plugin list` renders exactly the same table as Python (including rich's column
shrinking and ellipsis rules), and `proj convert` error panels are byte-identical.

## Build

```powershell
cd E:\RUST_LIBRESVIP\RustLibreSVIP
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo build --release
target\release\libresvip-cli.exe --help
```

## Usage

The command surface mirrors `python -m libresvip.cli`:

```powershell
libresvip-cli plugin list
libresvip-cli plugin detail svp
libresvip-cli conf list
libresvip-cli proj convert input.svp output.ustx
libresvip-cli proj split input.svp out_dir --output-ext ust --max-track-count 2
libresvip-cli proj merge a.svp b.svp --out-path merged.svp
```

Interactive conversions ask for the same options, middleware questions and
defaults as the Python CLI.

## Verification

Everything is verified against the live Python implementation (conda `study12`):

```powershell
$py = "E:\miniconda\envs\study12\python.exe"

# CLI parity: stdout, stderr, exit codes and settings.yml round-trips
& $py scripts\check_parity.py -v

# Format plugins: byte-identical dumps and identical loaded models
$env:GOLDEN_ONLY = "svp"      # optional single plugin
cargo test -p libresvip-plugins golden -- --nocapture

# Regenerate reference fixtures after a Python-side change
& $py scripts\gen_golden.py
```

Golden fixtures live in `testdata/golden/<plugin-id>/` and are produced by
`scripts/gen_golden.py`.

## Layout

```
crates/libresvip-core        models, math, config, options, plugin traits, i18n
crates/libresvip-plugins     one module per format (src/<plugin-id>.rs)
crates/libresvip-middlewares the six project middlewares
crates/libresvip-cli         the `libresvip-cli` binary
scripts/                     code generation, fixtures, parity harness
testdata/                    fixtures and golden references
```

See [PORTING.md](PORTING.md) for the contributor/agent guide.

## Performance

`scripts/bench.py` converts a 1.6 MiB project (10 tracks, 6000 notes, 4000-point
pitch curves) and compares both implementations (`--repeat 3`, medians):

| conversion | Python | Rust | speed-up | output |
| --- | --- | --- | --- | --- |
| json → json | 4.76 s | 0.058 s | **82×** | byte-identical |
| json → svp | 8.31 s | 0.072 s | **115×** | identical apart from random UUIDs |
| json → ust | 5.83 s | 0.064 s | **92×** | byte-identical |
| json → mid | 17.04 s | 0.096 s | **178×** | byte-identical |
| json → ufdata | 4.73 s | 0.057 s | **83×** | byte-identical |

```powershell
cargo build --release
& $py scripts\bench.py --repeat 5
```

## Known differences

* `--help` screens use clap's layout instead of typer's rich panels (same
  commands, arguments and option names).
* Runtime failures print `Error: …` instead of a Python traceback.
* `rpc server` and `mcp serve` are implemented; their per-request error payloads
  carry a plain error message instead of a Python traceback.
* Audio metadata probing understands RIFF/WAVE only (Python uses
  MediaInfo/libsndfile), so non-WAV backing tracks lose their duration.
* Where LibreSVIP 2.9.2 itself crashes (e.g. `xvsq` with an instrumental track,
  `vsqx` with parameter curves), the Rust port implements the evident intent and
  the parity harness reports `SKIP`.
* Formats that embed wall-clock values (ZIP DOS timestamps in `vpr`/`vog`/`vfp`,
  UUIDs/song ids in `svp`/`ppsf`/`svip3`/`ace`/`vvproj`, AES salts in
  `ps_project`) are classified `nondeterministic`: golden tests compare their
  content instead of raw bytes.
* `vshp` is import-only and the repository has no sample file; it is verified
  with Python-`construct`-built fixtures and byte-layout unit tests instead of a
  golden dump.
