# Real-project benchmark harness

The scripts behind [`docs/benchmark.md`](../../docs/benchmark.md). They measure
RustLibreSVIP, the Python reference (LibreSVIP 2.9.2) and UtaFormatix3 on
projects that already exist on disk, one conversion at a time, and they check
that the three engines agree on what the project contains.

Every script keeps a few absolute paths at the top (this checkout, the Python
environment, the UtaFormatix3 checkout). Edit those four constants to run
elsewhere.

## Pipeline

| step | script | what it does |
| --- | --- | --- |
| 1 | `scan_projects.py` | walks the drives and writes `scan.jsonl` (path, format, size, mtime) |
| 2 | `select_cases.py` | picks a balanced subset into `cases.json` (per format and size band, rust/uf3/py capability flags) |
| 3 | `runner.py` | runs the conversions serially, appends one row per conversion to `results.csv` (resumable) |
| 4 | `consistency.py cross` | every engine writes the same intermediate format, one common reader normalises it, then the note/pitch/tempo content is compared |
| 5 | `consistency.py roundtrip` | write format X, read it back, compare with the original: shows what each container keeps |
| 6 | `tolerance.py` | re-scores note agreement with a ±1/±2 tick tolerance (millisecond sources cannot hit the same tick grid) |
| 7 | `report.py` | turns the CSVs into `summary.md` / `summary.json` |

## UtaFormatix3

UtaFormatix3 is a Kotlin/JS library with no ready-made Node CLI, so this
directory builds one:

```bat
gradle -p <utaformatix3> --no-daemon -Pkotlin.js.yarn=false ^
       :core:browserProductionLibraryDistribution
python patch_uf3.py      :: rewrites the 9 require() calls into ESM
node uf3_cli.js <in> <out> <source-ext> <target>
```

`uf3_cli.js` prints the in-process parse/generate time as JSON so the benchmark
can separate Node start-up from the conversion itself, and `uf3_polyfill.mjs`
supplies the browser APIs the library expects (`FileReader`, an XML DOM, a
couple of DOM methods Node does not have).

## Read/write split and WebAssembly

`docs/benchmark.md` also reports where a conversion spends its time (reading the
source format versus writing the target) and how the wasm build compares. Those
numbers come from a second, smaller harness:

| step | script | what it does |
| --- | --- | --- |
| 8 | `cargo build --release -p libresvip-cli --bins --example phases` | builds the timing binary, which loads and dumps inside one process |
| 9 | `native_phases.py` | runs `phases.exe` once per (project, target) and writes `phases_native.csv` with `load_ms`/`dump_ms` |
| 10 | `wasm_run2.py` + `wasm_cli.js` | runs every conversion through the wasm module, one Node process each, and writes `results_wasm2.csv` |
| 11 | `warm_run.py` + `wasm_batch2.js` | converts one representative file per pair four times inside one native process and one wasm instance, for the continuous-conversion numbers (`warm_cold.csv`) |
| 12 | `analysis.py` | aggregates the CSVs into `report_data.json`, the markdown tables of `docs/benchmark.md` and the charts in `docs/images/benchmark/` |

`phases.exe` reads the input file once before starting its clock; part of the
corpus sits on a cloud-sync virtual drive, and a cold read of a 493 KB project
took 235 ms against 3.5 ms warm.

## Fairness rules

* one conversion process at a time, never two,
* every conversion starts a fresh process, so CLI/Node start-up is included in
  the measured wall time (UtaFormatix3 additionally reports its internal time),
* UtaFormatix3 is capped at 1.2 MB input, 200 KB for its XML exporters and
  120 s wall clock; without the caps a single large project costs minutes,
* the Python subset is limited to small inputs for the same reason,
* failures and timeouts stay in the CSV instead of being dropped.
