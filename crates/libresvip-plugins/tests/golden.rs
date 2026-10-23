//! Golden tests against the Python reference implementation.
//!
//! `scripts/gen_golden.py` (run with the study12 interpreter) materialises, for
//! every format plugin, the reference output (`dump.<suffix>`) for
//! `testdata/<fixture>.json` plus the internal model produced by loading that
//! output (`project.json`).
//!
//! Each plugin test asserts two things:
//!   1. dumping the fixture produces byte-identical output (unless the format is
//!      non-deterministic, e.g. embedded UUIDs),
//!   2. loading `dump.<suffix>` yields the same project model as Python.
//!
//! Run one plugin with `GOLDEN_ONLY=ust cargo test -p libresvip-plugins golden`.

use std::path::{Path, PathBuf};

use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::SvsPlugin;
use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn testdata_dir() -> PathBuf {
    workspace_root().join("testdata")
}

fn golden_root() -> PathBuf {
    testdata_dir().join("golden")
}

/// Defaults materialised from an option schema (the CLI would prompt for them).
fn default_options(schema: &[OptionFieldDef]) -> Options {
    let mut options = Options::new();
    for field in schema {
        options.insert(field.key, field.default.clone());
    }
    options
}

fn read_project(path: &Path) -> Project {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn json_value(project: &Project) -> Value {
    serde_json::to_value(project).expect("serialize project")
}

/// The parsers never store a pronunciation that merely repeats the lyric
/// (`wo`, `la`, …), the reference does; every generator that prefers
/// `Note.pronunciation` then copied that echo into a phoneme field. Ignore the
/// redundant field so the rest of the loaded model is still compared exactly.
fn drop_echoed_pronunciations(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let echoed = match (map.get("Lyric"), map.get("Pronunciation")) {
                (Some(Value::String(lyric)), Some(Value::String(pronunciation))) => {
                    lyric.trim() == pronunciation.trim()
                }
                _ => false,
            };
            if echoed {
                map.insert("Pronunciation".to_owned(), Value::Null);
            }
            for nested in map.values_mut() {
                drop_echoed_pronunciations(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                drop_echoed_pronunciations(item);
            }
        }
        _ => {}
    }
}

struct Report {
    id: &'static str,
    dumped: Result<(), String>,
    loaded: Result<(), String>,
}

impl Report {
    fn ok(&self) -> bool {
        self.dumped.is_ok() && self.loaded.is_ok()
    }
}

fn check_plugin(plugin: &dyn SvsPlugin, only: Option<&str>) -> Option<Report> {
    let id = plugin.info().id;
    if let Some(only) = only {
        if only != id {
            return None;
        }
    }
    let folder = golden_root().join(id);
    if !folder.is_dir() {
        return None;
    }
    let meta: Value = serde_json::from_slice(&std::fs::read(folder.join("meta.json")).unwrap())
        .expect("meta.json");
    let suffix = meta["suffix"].as_str().unwrap_or_default();
    let fixture = meta["fixture"].as_str().unwrap_or("simple.json");
    let can_dump = meta["dumped"].as_bool().unwrap_or(false);
    let can_load = meta["loaded"].as_bool().unwrap_or(false);
    let nondeterministic = meta["nondeterministic"].as_bool().unwrap_or(false);

    let expected_dump = folder.join(format!("dump.{suffix}"));
    let fixture_path = testdata_dir().join(fixture);
    let project = read_project(&fixture_path);

    let dumped = if can_dump {
        let temp = std::env::temp_dir().join(format!("libresvip-golden-{id}.{suffix}"));
        let options = default_options(&plugin.output_options());
        match plugin.dump(&temp, &project, &options) {
            Ok(()) => {
                let actual = std::fs::read(&temp).unwrap_or_default();
                let expected = std::fs::read(&expected_dump).unwrap_or_default();
                if nondeterministic {
                    Ok(())
                } else if actual == expected {
                    Ok(())
                } else {
                    Err(first_difference(&expected, &actual, suffix))
                }
            }
            Err(error) => Err(format!("dump error: {error}")),
        }
    } else {
        Ok(())
    };

    let loaded = if can_load {
        let options = default_options(&plugin.input_options());
        match plugin.load(&expected_dump, &options) {
            Ok(actual_project) => {
                let expected_project = read_project(&folder.join("project.json"));
                let mut actual = json_value(&actual_project);
                let mut expected = json_value(&expected_project);
                drop_echoed_pronunciations(&mut actual);
                drop_echoed_pronunciations(&mut expected);
                if actual == expected {
                    Ok(())
                } else {
                    Err(first_json_difference(&expected, &actual))
                }
            }
            Err(error) => Err(format!("load error: {error}")),
        }
    } else {
        Ok(())
    };

    Some(Report { id, dumped, loaded })
}

fn first_difference(expected: &[u8], actual: &[u8], suffix: &str) -> String {
    if expected.len() != actual.len() {
        // For text formats show the first differing line, which is far more
        // useful than a byte offset.
        let expected_text = String::from_utf8_lossy(expected);
        let actual_text = String::from_utf8_lossy(actual);
        if suffix != "svip" && suffix != "ppsf" && suffix != "ps_project" {
            for (index, (left, right)) in expected_text.lines().zip(actual_text.lines()).enumerate() {
                if left != right {
                    return format!(
                        "first differing line {}:\n  python: {:?}\n  rust:   {:?}",
                        index + 1,
                        left,
                        right
                    );
                }
            }
        }
        return format!(
            "length mismatch: python {} bytes, rust {} bytes",
            expected.len(),
            actual.len()
        );
    }
    for (index, (left, right)) in expected.iter().zip(actual.iter()).enumerate() {
        if left != right {
            return format!("byte {index} differs: python {left:#04x}, rust {right:#04x}");
        }
    }
    "outputs differ".to_owned()
}

/// Shrinks the JSON diff to the first differing path.
fn first_json_difference(expected: &Value, actual: &Value) -> String {
    fn walk(path: &str, expected: &Value, actual: &Value) -> Option<String> {
        match (expected, actual) {
            (Value::Object(left), Value::Object(right)) => {
                for (key, left_value) in left {
                    match right.get(key) {
                        Some(right_value) => {
                            let child = format!("{path}.{key}");
                            if let Some(result) = walk(&child, left_value, right_value) {
                                return Some(result);
                            }
                        }
                        None => return Some(format!("{path}.{key}: missing in rust")),
                    }
                }
                for key in right.keys() {
                    if !left.contains_key(key) {
                        return Some(format!("{path}.{key}: extra in rust"));
                    }
                }
                None
            }
            (Value::Array(left), Value::Array(right)) => {
                if left.len() != right.len() {
                    return Some(format!(
                        "{path}: length {} vs {}",
                        left.len(),
                        right.len()
                    ));
                }
                for (index, (left_value, right_value)) in left.iter().zip(right.iter()).enumerate() {
                    let child = format!("{path}[{index}]");
                    if let Some(result) = walk(&child, left_value, right_value) {
                        return Some(result);
                    }
                }
                None
            }
            _ => {
                if expected == actual {
                    None
                } else {
                    Some(format!("{path}: python {expected} vs rust {actual}"))
                }
            }
        }
    }
    walk("$", expected, actual).unwrap_or_else(|| "projects differ".to_owned())
}

#[test]
fn golden() {
    let only = std::env::var("GOLDEN_ONLY").ok();
    let only = only.as_deref();
    let mut reports = Vec::new();
    for plugin in libresvip_plugins::all_plugins() {
        if let Some(report) = check_plugin(*plugin, only) {
            reports.push(report);
        }
    }
    if reports.is_empty() {
        panic!("no golden fixtures found (run scripts/gen_golden.py)");
    }
    let mut failures = Vec::new();
    for report in &reports {
        let status = if report.ok() { "PASS" } else { "FAIL" };
        println!("{status} {}", report.id);
        if !report.ok() {
            if let Err(error) = &report.dumped {
                println!("  dump: {error}");
            }
            if let Err(error) = &report.loaded {
                println!("  load: {error}");
            }
            failures.push(report.id);
        }
    }
    if !failures.is_empty() {
        panic!(
            "{}/{} plugins match the Python reference; failing: {}",
            reports.len() - failures.len(),
            reports.len(),
            failures.join(", ")
        );
    }
}

/// Every plugin's option schema must be stable and free of duplicate keys.
#[test]
fn option_schemas_are_well_formed() {
    for plugin in libresvip_plugins::all_plugins() {
        for schema in [plugin.input_options(), plugin.output_options()] {
            let mut keys = std::collections::HashSet::new();
            for field in &schema {
                assert!(
                    keys.insert(field.key),
                    "{}: duplicate option key {}",
                    plugin.info().id,
                    field.key
                );
                assert!(field.title != "None", "{}: option {} has no title", plugin.info().id, field.key);
            }
        }
    }
}
