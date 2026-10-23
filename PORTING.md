# Porting LibreSVIP formats to Rust — contributor & agent guide

The Python implementation in `../LibreSVIP` (version **2.9.2**) is the single
source of truth. This workspace is a pure-Rust, CLI-only port with the same
command surface and the same conversion behaviour.

## Environment

| what | command |
| --- | --- |
| Rust | `cargo build` / `cargo test` (MSVC toolchain, `~/.cargo/bin` on PATH) |
| Python reference | `E:\miniconda\envs\study12\python.exe` with `PYTHONPATH=E:\RUST_LIBRESVIP\LibreSVIP` |
| Debug binary | `target\debug\libresvip-cli.exe` |
| Regenerate plugin metadata | `python scripts/gen_plugin_infos.py` |
| Regenerate translation assets | `python scripts/sync_assets.py` |
| Regenerate golden fixtures | `python scripts/gen_golden.py [--only <id>]` |
| CLI parity suite | `python scripts/check_parity.py [-v] [--only <name>]` |

Golden fixtures live in `testdata/golden/<plugin-id>/`:

* `dump.<suffix>` — Python output for `testdata/simple.json` with default options
* `project.json` — Python's internal model after loading that dump
* `meta.json` — `dumped` / `loaded` / `nondeterministic` flags

## Layout

```
crates/libresvip-core        models, tick math, time sync, config, options, plugin traits
crates/libresvip-plugins     one module per format: src/<plugin-id>.rs (+ src/<id>/ submodules)
crates/libresvip-middlewares the six project middlewares
crates/libresvip-cli         the `libresvip-cli` binary
```

Format modules are named after the plugin identifier (`svp`, `ust`, `ps_project`, …)
and each exports `pub static PLUGIN: <Type>` implementing `SvsPlugin`.

## Core API (read these files before porting)

* `crates/libresvip-core/src/model/base.rs` — `Project`, `Track`, `SingingTrack`,
  `InstrumentalTrack`, `Note`, `SongTempo`, `TimeSignature`, `ParamCurve`,
  `Params`, `VibratoParam`, `Phones`. Field order and serde aliases mirror the
  pydantic models; use the accessor helpers (`track.as_singing()`, …).
* `crates/libresvip-core/src/model/point.rs` — `Point` (serialises as `[x, y]`).
* `crates/libresvip-core/src/options.rs` — `OptionFieldDef`, `Options`,
  option mixins (`select_single_track()`, `static_tempo()`, …).
* `crates/libresvip-core/src/plugin.rs` — `SvsPlugin`, `PluginInfo`.
* `crates/libresvip-core/src/core/tick_counter.rs`, `time_sync.rs`,
  `model/reset_time_axis.rs` — tick/tempo helpers.
* `crates/libresvip-plugins/src/json.rs` — the smallest complete reference
  implementation.
* `crates/libresvip-plugins/src/plugin_infos.rs` — generated metadata
  (`INFO_<ID>`, `INFO_<ID>_VERSION`); never edit by hand.

## Porting one plugin

1. Read the Python plugin: `libresvip/plugins/<id>/` — usually
   `options.py`, `model.py`, `<id>_parser.py`, `<id>_generator.py`,
   `<id>_converter.py`, plus helpers and the `.yapsy-plugin` metadata.
2. Replace `crates/libresvip-plugins/src/<id>.rs` (currently a stub) with a real
   `SvsPlugin` implementation. Create `src/<id>/` submodules for the model,
   parser and generator when that keeps files readable.
3. Port `options.py` **field by field**: keys, titles, descriptions, defaults and
   order all matter, because `plugin detail` prints them and the CLI prompts for
   them. Use `OptionFieldDef::bool/int/float/string/enumerated(...)`; enums need
   the member **name**, value and translated title.
4. Port the parser (`load`) and generator (`dump`) faithfully.
5. Verify:

   ```powershell
   $env:GOLDEN_ONLY = "ust"
   cargo test -p libresvip-plugins golden -- --nocapture
   ```

   The test compares (a) `dump` bytes against Python's output and (b) the project
   model after `load`. Iterate until it passes.
6. Also check `libresvip-cli.exe plugin detail <id>` against
   `python -m libresvip.cli plugin detail <id>` (same console encoding).

## Behaviour rules

* **Faithfulness beats cleanliness.** Reproduce quirks, including rounding
  (`Python round` = ties-to-even → `f64::round_ties_even()`), clamping, and
  ordering of points/notes. If Python crashes on an input, returning an error is
  acceptable — but do not invent behaviour.
* Keep every user-visible string identical to the Python message id (English).
* `Option` fields serialise as `null` (no `skip_serializing_if`).
* Read unknown JSON keys leniently (serde ignores them by default); accept both
  alias and snake_case field names on input (`#[serde(rename = "Camel", alias = "snake")]`).
* File formats: write the same bytes Python writes, including trailing newlines,
  BOMs and encodings. Use `encoding_rs` for legacy charsets.
* Errors: return `LibreSvipError` (`msg`, `Validation`, `NotImplemented`, …).
  Never panic in library code.
* Do not edit files outside your assignment. Shared helpers go in your own
  module; if something really belongs in `libresvip-core`, report it instead.

## CLI surface (already implemented)

`conf list|set`, `plugin list|detail|toggle`, `proj convert|split|merge`
(interactive prompts read stdin), `rpc server`, `mcp serve` (stubs).
`python scripts/check_parity.py` diffs the Rust CLI against the Python CLI for
stdout, stderr and exit codes.
