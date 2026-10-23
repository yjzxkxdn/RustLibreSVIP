# RustLibreSVIP

Pure-Rust port of [LibreSVIP](https://github.com/SoulMelody/LibreSVIP) **2.9.2** —
CLI and desktop GUI. The Python implementation in `../LibreSVIP` is the source
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
| Desktop GUI (`libresvip-gui`, egui/eframe) | ported, see [Desktop GUI](#desktop-gui) |

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

## Desktop GUI

`crates/libresvip-gui` rebuilds the Python project's Qt Quick interface with
egui/eframe: the same layout (format card, task list, advanced settings, output
settings), the same Material light/dark palette and the same conversion
behaviour (temporary output per task, conflict policy, warning badges).

```powershell
cargo run -p libresvip-gui --release
# or start the binary directly
target\release\libresvip-gui.exe
```

Files can be passed on the command line or dropped onto the window:

```powershell
target\release\libresvip-gui.exe song.svp another.svp
```

On Windows, an unsigned exe started from Explorer can trigger the shell's
"Open File - Security Warning" or be launched in a security sandbox (low
integrity level, which blocks every file write). Start it from a console
instead — `run-gui.cmd` in the repository root does exactly that:

```powershell
.\run-gui.cmd
# or
& .\target\release\libresvip-gui.exe
```

A checkout that lives in a low-integrity (sandboxed) directory — for example a
workspace created by an agent harness — stamps that label, a sandbox SID and a
"Low mandatory level" onto every file built there, and Windows then treats the
exe as untrusted. `install-gui.ps1` copies the binary to
`%LOCALAPPDATA%\Programs\RustLibreSVIP`, resets its integrity level and creates
a Start Menu shortcut:

```powershell
powershell -ExecutionPolicy Bypass -File .\install-gui.ps1 -Build
```

(The `-ExecutionPolicy Bypass` is only needed when the machine's policy blocks
unsigned scripts; `install-gui.ps1` is a plain text file you can also read and
run line by line.)

| feature | notes |
| --- | --- |
| format combos, auto-detection, swap | read/write plugins from the shared registry |
| task list: rename stem, change extension, remove | direct / merge / split modes |
| option forms for input, output and the six middlewares | generated from the same `OptionFieldDef` schemas the CLI uses |
| output folder, conflict policy, "open folder when done" | reads and writes the same `settings.yml` as the CLI and the Python GUI |
| light / dark / system theme, zh_CN / en_US / ja_JP / de_DE | translations come from the upstream gettext catalogs |
| drag & drop, Ctrl+O, Ctrl+Enter, Ctrl+R, Ctrl+Tab, F1 | |

```powershell
cargo test -p libresvip-gui
```

The tests drive the real pipeline: a direct conversion re-loads to the project
recorded in `testdata/golden/acep/project.json`, and split/merge modes produce
loadable projects.

Not ported (documented differences):

* the lyric-replacement-rule editor in the settings dialog — rules in
  `settings.yml` are used, but cannot be edited in the GUI;
* "Check for Updates" only shows a notice (no network access);
* the QML compact layout for narrow windows — the split view is always used;
* colour options are edited as text, without a colour picker.

## WebAssembly

`crates/libresvip-web` builds the same engine for `wasm32-unknown-unknown`. The
module has **no imports** — no WASI, no wasm-bindgen — so a plain
`WebAssembly.instantiate(bytes, {})` is enough, and `web/index.html` drives it
through the small C ABI documented in `crates/libresvip-web/src/lib.rs`.

```powershell
rustup target add wasm32-unknown-unknown
cargo build --profile wasm -p libresvip-web --target wasm32-unknown-unknown
node web/build-web.mjs
```

`web/build-web.mjs` copies the module to `web/libresvip_web.wasm` and writes
`web/libresvip-single.html`, the same page with the gzip-compressed module
inlined so it opens straight from disk (no local server, no bundler).

| artifact | size |
| --- | --- |
| `libresvip_web.wasm` | 6.32 MiB |
| gzip / brotli | 2.17 MiB / 1.60 MiB |
| `libresvip-single.html` | 2.90 MiB |

Serve the module with `Content-Type: application/wasm` plus a pre-compressed
`.br` next to it (`Content-Encoding: br`) and a long `Cache-Control`: the
browser downloads it once and runs every conversion locally, so the server only
serves a static file. The wasm path is exercised in Node:
`ustx → acep` on a 4 minute project takes ≈ 0.4 s (native: 0.15 s) and produces
the same project as the CLI.

The web entry point converts one file with the plugins' default options;
`split`, `merge` and custom options are CLI/GUI-only for now.

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
crates/libresvip-gui         the `libresvip-gui` desktop app (egui/eframe)
crates/libresvip-web         the wasm32 entry point (`cdylib`, C ABI)
web/                         browser demo page + `build-web.mjs`
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

Real projects hit a different code path: the `.acep` writer smooths a 119-tap
kernel over the whole song for every vocal pattern, so `ustx → acep` is the
slowest conversion of both implementations. Measured on a 4 minute OpenUtau
project (579 notes, 21 patterns, `--repeat 3`, medians):

| conversion | Python | Rust | speed-up | output |
| --- | --- | --- | --- | --- |
| ustx → acep | 24.66 s | 0.147 s | **168×** | identical apart from the random track colours |

```powershell
cargo build --release
& $py scripts\bench.py --repeat 5
# a real OpenUtau song (any .ustx file works, --input accepts a path)
& $py scripts\bench.py --pairs ustx:acep --input D:\path\to\song.ustx --repeat 3
```

Rust used to be as slow as Python on this file: `math.sumprod` was ported as a
per-product Shewchuk expansion (≈ 119 products × growing partial list per sample
instead of CPython's constant-size triple-length accumulator), and every pattern
re-convolved the timeline from second 0. See `acep/music_math.rs` and
`acep/base_pitch_curve.rs`.

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
