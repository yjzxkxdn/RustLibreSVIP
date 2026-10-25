//! Times the read and write halves of one conversion separately.
//!
//! ```text
//! cargo build --release -p libresvip-cli --example phases
//! phases <input file> <output file>
//! phases --jobs <file.tsv>
//! ```
//!
//! It resolves both plugins by file suffix and uses their default options, the
//! same way `libresvip-cli proj convert` does without a prompt, and prints one
//! JSON line per conversion on stdout:
//!
//! ```json
//! {"i":0,"load_ms":12.34,"dump_ms":56.78,"out_bytes":921754}
//! ```
//!
//! `--jobs` reads a TSV with `input<TAB>output<TAB>source id<TAB>target id` per
//! line (the ids are only used by the WebAssembly harness, which needs them;
//! here the plugins come from the suffixes) and converts all of them inside one
//! process. That is the warm path: a desktop GUI or an editor session converts
//! many projects without ever paying process start-up again.
//!
//! The benchmark harness (`scripts/bench_corpus/`) drives the single-pair form
//! once per conversion so that every cold measurement pays a process, and the
//! `--jobs` form once per warm round.

use std::path::Path;
use std::time::Instant;

use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_plugins::plugin_by_suffix;

fn defaults(schema: &[OptionFieldDef]) -> Options {
    Options::from_pairs(
        schema
            .iter()
            .map(|field| (field.key.to_owned(), field.default.clone())),
    )
}

fn usage() -> ! {
    eprintln!("usage: phases <input file> <output file>");
    eprintln!("       phases --jobs <jobs.tsv>");
    std::process::exit(2);
}

fn suffix_of(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// One conversion, timed in two halves.
fn convert_once(input: &Path, output: &Path) -> Result<(f64, f64, u64), String> {
    let input_plugin = plugin_by_suffix(&suffix_of(input))
        .ok_or_else(|| format!("no reader for {}", input.display()))?;
    let output_plugin = plugin_by_suffix(&suffix_of(output))
        .ok_or_else(|| format!("no writer for {}", output.display()))?;
    if !output_plugin.writable() {
        return Err(format!("{} cannot be written", output.display()));
    }

    let load_started = Instant::now();
    let project = input_plugin
        .load(input, &defaults(&input_plugin.input_options()))
        .map_err(|error| error.to_string())?;
    let load = load_started.elapsed().as_secs_f64() * 1000.0;

    let dump_started = Instant::now();
    output_plugin
        .dump(output, &project, &defaults(&output_plugin.output_options()))
        .map_err(|error| error.to_string())?;
    let dump = dump_started.elapsed().as_secs_f64() * 1000.0;

    let out_bytes = std::fs::metadata(output)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    Ok((load, dump, out_bytes))
}

/// A TSV of `input<TAB>output<TAB>source id<TAB>target id`.
fn run_jobs(path: &str) -> i32 {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("phases: {error}");
            return 1;
        }
    };
    let mut failures = 0;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let (Some(input), Some(output)) = (fields.next(), fields.next()) else {
            println!("{{\"i\":{index},\"error\":\"malformed job line\"}}");
            failures += 1;
            continue;
        };
        match convert_once(Path::new(input), Path::new(output)) {
            Ok((load, dump, out_bytes)) => println!(
                "{{\"i\":{index},\"load_ms\":{load:.3},\"dump_ms\":{dump:.3},\"out_bytes\":{out_bytes}}}"
            ),
            Err(message) => {
                println!("{{\"i\":{index},\"error\":\"{}\"}}", escape(&message));
                failures += 1;
            }
        }
    }
    println!("{{\"jobs\":{}}}", text.lines().filter(|line| !line.trim().is_empty()).count());
    if failures > 0 {
        1
    } else {
        0
    }
}

/// Minimal JSON string escaping for error messages.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' | '\t' => out.push(' '),
            character => out.push(character),
        }
    }
    out
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() == 2 && arguments[0] == "--jobs" {
        std::process::exit(run_jobs(&arguments[1]));
    }
    let [input, output] = arguments.as_slice() else {
        usage();
    };
    match convert_once(Path::new(input), Path::new(output)) {
        Ok((load, dump, out_bytes)) => println!(
            "{{\"load_ms\":{load:.3},\"dump_ms\":{dump:.3},\"out_bytes\":{out_bytes}}}"
        ),
        Err(message) => {
            eprintln!("phases: {message}");
            std::process::exit(1);
        }
    }
}
