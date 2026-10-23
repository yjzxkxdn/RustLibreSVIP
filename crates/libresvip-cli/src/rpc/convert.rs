//! Port of `libresvip.cli.rpc.server.Conversion.convert` and `convert_one_group`.
//!
//! Python keeps its temporary files in a `memory://` filesystem; this port uses
//! a per-request directory under the OS temp directory. The observable behaviour
//! is the same: the plugin receives an extension-less file at `<group>/<index>`,
//! warnings are captured with [`CatchWarnings`], and load/dump/middleware errors
//! become per-group [`SingleConversionResult`] failures. Errors outside those
//! catch blocks (unknown formats, unreadable/unwritable plugins, empty groups,
//! split with a negative track count, merge with fewer than two projects)
//! propagate as gRPC errors, like the unhandled Python exceptions do.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use libresvip_core::core::warning_types::CatchWarnings;
use libresvip_core::error::LibreSvipError;
use libresvip_core::model::base::{Project, Track};
use libresvip_core::options::{OptionFieldDef, OptionKind, OptionValue, Options};
use libresvip_core::plugin::{Middleware, SvsPlugin};
use libresvip_middlewares::all_middlewares;
use libresvip_plugins::{enabled_plugins, plugin_by_suffix};
use serde_json::Value;

use super::messages::{
    ConversionGroup, ConversionRequest, ConversionResponse, GroupResultEntry,
    MiddlewareOptionsEntry, SingleConversionResult,
};
use super::schema::integer_enum;

const MODE_SPLIT: i32 = 1;
const MODE_MERGE: i32 = 2;

/// `get_svs_plugin_by_id(id) or get_svs_plugin_by_suffix(id)`.
pub fn find_plugin(value: &str) -> Option<&'static dyn SvsPlugin> {
    enabled_plugins()
        .into_iter()
        .find(|plugin| plugin.info().id == value)
        .or_else(|| plugin_by_suffix(value))
}

/// Runs one `Convert` request. `Err` becomes an UNKNOWN gRPC status, matching
/// grpclib's handling of uncaught exceptions in the Python server.
pub fn convert(request: &ConversionRequest) -> Result<ConversionResponse, String> {
    let input_plugin = find_plugin(&request.input_format).ok_or_else(|| {
        "AttributeError: 'NoneType' object has no attribute 'input_option_cls'".to_owned()
    })?;
    let output_plugin = find_plugin(&request.output_format).ok_or_else(|| {
        "AttributeError: 'NoneType' object has no attribute 'output_option_cls'".to_owned()
    })?;
    // `ReadOnlyConverterMixin` / `WriteOnlyConverterMixin` option classes are the
    // bare pydantic `BaseModel`, so Python raises before touching any group.
    if !input_plugin.readable() {
        return Err(
            "pydantic.errors.PydanticUserError: Pydantic models should inherit from BaseModel, \
             BaseModel cannot be instantiated directly"
                .to_owned(),
        );
    }
    if !output_plugin.writable() {
        return Err(
            "pydantic.errors.PydanticUserError: Pydantic models should inherit from BaseModel, \
             BaseModel cannot be instantiated directly"
                .to_owned(),
        );
    }

    let input_options = parse_options(&request.input_options, &input_plugin.input_options());
    let output_options = parse_options(&request.output_options, &output_plugin.output_options());
    let middlewares = all_middlewares();

    let root = unique_root();
    let mut entries = Vec::with_capacity(request.groups.len());
    for group in &request.groups {
        match convert_one_group(
            &root,
            request,
            group,
            input_plugin,
            output_plugin,
            &input_options,
            &output_options,
            &middlewares,
        ) {
            Ok((group_id, result)) => entries.push(GroupResultEntry {
                key: group_id,
                value: Some(result),
            }),
            Err(error) => {
                let _ = fs::remove_dir_all(&root);
                return Err(error);
            }
        }
    }
    let _ = fs::remove_dir_all(&root);
    Ok(ConversionResponse {
        group_results: entries,
    })
}

#[allow(clippy::too_many_arguments)]
fn convert_one_group(
    root: &Path,
    request: &ConversionRequest,
    group: &ConversionGroup,
    input_plugin: &'static dyn SvsPlugin,
    output_plugin: &'static dyn SvsPlugin,
    input_options: &Options,
    output_options: &Options,
    middlewares: &[&'static dyn Middleware],
) -> Result<(String, SingleConversionResult), String> {
    let mode = request.mode;
    let group_dir = group_directory(root, &group.group_id)?;
    create_group_directory(root, &group_dir)?;

    let mut result = SingleConversionResult::default();
    let mut project: Option<Project> = None;

    if mode == MODE_MERGE {
        let mut child_projects = Vec::new();
        let mut failed = false;
        for (index, content) in group.file_contents.iter().enumerate() {
            let child_path = group_dir.join(index.to_string());
            fs::write(&child_path, content).map_err(|error| error.to_string())?;
            let collector = CatchWarnings::start();
            let loaded = input_plugin.load(&child_path, input_options);
            let warnings = collector.finish();
            match loaded {
                Ok(loaded) => {
                    append_warnings(&mut result, warnings);
                    child_projects.push(loaded);
                }
                Err(error) => {
                    failed = true;
                    result.success = false;
                    result.error_message = error_text(&error);
                    break;
                }
            }
        }
        if !failed {
            // `Project.merge_projects` validates the child list outside the
            // Python try/except, so these errors abort the whole RPC.
            project =
                Some(Project::merge_projects(child_projects).map_err(|error| error_text(&error))?);
        }
    } else {
        if group.file_contents.is_empty() {
            return Err("IndexError: list index out of range".to_owned());
        }
        let child_path = group_dir.join("0");
        fs::write(&child_path, &group.file_contents[0]).map_err(|error| error.to_string())?;
        let collector = CatchWarnings::start();
        let loaded = input_plugin.load(&child_path, input_options);
        let warnings = collector.finish();
        match loaded {
            Ok(loaded) => {
                append_warnings(&mut result, warnings);
                project = Some(loaded);
            }
            Err(error) => {
                result.success = false;
                result.error_message = error_text(&error);
            }
        }
    }

    // Python's memory filesystem `rmdir()` removes non-empty directories.
    let _ = fs::remove_dir_all(&group_dir);

    let mut current = project.take();
    for MiddlewareOptionsEntry { key, value } in &request.middleware_options {
        let Some(project_value) = current.take() else {
            break;
        };
        let Some(middleware) = middlewares
            .iter()
            .find(|middleware| middleware.info().id == key)
        else {
            // Unknown middleware ids are silently skipped, but the project
            // still flows through the pipeline.
            current = Some(project_value);
            continue;
        };
        let process_options = parse_options(value, &middleware.process_options());
        let collector = CatchWarnings::start();
        let processed = middleware.process(project_value, &process_options);
        let warnings = collector.finish();
        match processed {
            Ok(processed) => {
                append_warnings(&mut result, warnings);
                current = Some(processed);
            }
            Err(error) => {
                result.success = false;
                result.error_message = error_text(&error);
                current = None;
                break;
            }
        }
    }
    let project = current;

    if let Some(project) = project {
        create_group_directory(root, &group_dir)?;
        if mode == MODE_SPLIT {
            // `more_itertools.chunked(iterable, 0)` yields no chunks and raises
            // for negative sizes; `split_tracks` still validates the presence
            // of singing tracks first.
            if request.max_track_count < 0 {
                return Err("ValueError: n must be at least 1".to_owned());
            }
            let mut complete = true;
            if request.max_track_count == 0 {
                if !project.track_list.iter().any(Track::is_singing) {
                    return Err(error_text(&LibreSvipError::NoSingingTracks));
                }
            } else {
                let sub_projects = project
                    .split_tracks(request.max_track_count as usize)
                    .map_err(|error| error_text(&error))?;
                for (index, sub_project) in sub_projects.iter().enumerate() {
                    let child_path = group_dir.join(index.to_string());
                    let collector = CatchWarnings::start();
                    let dumped = output_plugin.dump(&child_path, sub_project, output_options);
                    let warnings = collector.finish();
                    match dumped {
                        Ok(()) => {
                            append_warnings(&mut result, warnings);
                            match fs::read(&child_path) {
                                Ok(bytes) => result.file_contents.push(bytes),
                                Err(error) => {
                                    result.success = false;
                                    result.error_message = error.to_string();
                                    complete = false;
                                    break;
                                }
                            }
                        }
                        Err(error) => {
                            result.success = false;
                            result.error_message = error_text(&error);
                            complete = false;
                            break;
                        }
                    }
                }
            }
            if complete {
                result.success = true;
            }
        } else {
            let child_path = group_dir.join("0");
            // Unlike the SPLIT branch, Python's non-split dump is not wrapped
            // in `CatchWarnings`; warnings go to the logger and are not part of
            // the response.
            let dumped = output_plugin.dump(&child_path, &project, output_options);
            match dumped {
                Ok(()) => match fs::read(&child_path) {
                    Ok(bytes) => {
                        result.file_contents.push(bytes);
                        result.success = true;
                    }
                    Err(error) => {
                        result.success = false;
                        result.error_message = error.to_string();
                    }
                },
                Err(error) => {
                    result.success = false;
                    result.error_message = error_text(&error);
                }
            }
        }
        let _ = fs::remove_dir_all(&group_dir);
    }

    Ok((group.group_id.clone(), result))
}

/// `CatchWarnings.output` —several warnings are joined with newlines and
/// appended as a single entry.
fn append_warnings(result: &mut SingleConversionResult, warnings: Vec<String>) {
    if !warnings.is_empty() {
        result.warning_messages.push(warnings.join("\n"));
    }
}

/// Python's `traceback.format_exc()` is not reproducible from Rust; report the
/// exception message instead, prefixed with the Python exception class where a
/// direct counterpart exists.
fn error_text(error: &LibreSvipError) -> String {
    match error {
        LibreSvipError::InvalidFileType => {
            "libresvip.core.exceptions.InvalidFileTypeError: invalid file type".to_owned()
        }
        LibreSvipError::UnsupportedProjectVersion(version) => format!(
            "libresvip.core.exceptions.UnsupportedProjectVersionError: unsupported project version: {version}"
        ),
        LibreSvipError::NoTrack => {
            "libresvip.core.exceptions.NoTrackError: no track found".to_owned()
        }
        LibreSvipError::NotesOverlapped => {
            "libresvip.core.exceptions.NotesOverlappedError: notes overlapped".to_owned()
        }
        LibreSvipError::Params(message) => {
            format!("libresvip.core.exceptions.ParamsError: params error: {message}")
        }
        LibreSvipError::NotImplemented => "NotImplementedError: not implemented".to_owned(),
        other => other.to_string(),
    }
}

fn group_directory(root: &Path, group_id: &str) -> Result<PathBuf, String> {
    // fsspec's memory filesystem treats the id as a POSIX path: a leading `/`
    // is relative to the memory root, `.` components vanish and `mkdir()`
    // creates missing parents. Mirror that (without allowing escapes) because
    // this port writes to a real temp directory.
    let mut relative = PathBuf::new();
    for component in Path::new(group_id).components() {
        match component {
            Component::Normal(name) => relative.push(name),
            Component::CurDir | Component::RootDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                return Err(format!(
                    "FileNotFoundError: invalid conversion group id {group_id:?}"
                ));
            }
        }
    }
    if relative.as_os_str().is_empty() {
        // Python's `memory://` root already exists, so `mkdir()` raises.
        return Err("FileExistsError: memory://".to_owned());
    }
    Ok(root.join(relative))
}

/// `group_path.mkdir()` on fsspec's memory filesystem also creates missing
/// parents; the final component must not already exist.
fn create_group_directory(root: &Path, group_dir: &Path) -> Result<(), String> {
    if let Some(parent) = group_dir.parent() {
        if parent != root {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    fs::create_dir(group_dir).map_err(|error| error.to_string())
}

fn unique_root() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let root = std::env::temp_dir().join(format!(
        "libresvip-rpc-{}-{counter}-{nanos}",
        std::process::id()
    ));
    // `create_dir_all` failures surface later as a per-request gRPC error.
    let _ = fs::create_dir_all(&root);
    root
}

// ---------------------------------------------------------------------------
// Option JSON validation
// ---------------------------------------------------------------------------

/// `model_validate_json(raw)`, falling back to `Model()` (all defaults) exactly
/// like `Conversion.convert`: any validation error discards the whole payload.
fn parse_options(raw: &str, schema: &[OptionFieldDef]) -> Options {
    let defaults = default_options(schema);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return defaults;
    }
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return defaults;
    };
    let Value::Object(map) = value else {
        return defaults;
    };
    let mut options = defaults;
    for field in schema {
        let Some(raw_value) = map.get(field.key) else {
            continue;
        };
        let Some(option_value) = validate_field(raw_value, field) else {
            // Pydantic rejects the whole model, so every field is defaulted.
            return default_options(schema);
        };
        options.insert(field.key, option_value);
    }
    options
}

fn default_options(schema: &[OptionFieldDef]) -> Options {
    let mut options = Options::new();
    for field in schema {
        options.insert(field.key, field.default.clone());
    }
    options
}

fn validate_field(value: &Value, field: &OptionFieldDef) -> Option<OptionValue> {
    match &field.kind {
        OptionKind::Bool => validate_bool(value).map(OptionValue::Bool),
        OptionKind::Int => validate_int(value).map(OptionValue::Int),
        OptionKind::Float => validate_float(value).map(OptionValue::Float),
        OptionKind::Str => match value {
            Value::String(text) => Some(OptionValue::Str(text.clone())),
            _ => None,
        },
        OptionKind::Opaque { .. } => match value {
            Value::String(text) => Some(OptionValue::Str(text.clone())),
            _ => None,
        },
        OptionKind::Enum { variants, .. } => {
            let is_integer = integer_enum(variants);
            let matched = match value {
                Value::String(text) => variants.iter().find(|variant| variant.value == text),
                Value::Number(number) if is_integer => number.as_i64().and_then(|number| {
                    variants
                        .iter()
                        .find(|variant| variant.value.parse::<i64>() == Ok(number))
                }),
                _ => None,
            }?;
            // Python stores enum members; plugins in both implementations read
            // the member value, so the canonical string form is what matters.
            Some(OptionValue::Str(matched.value.to_owned()))
        }
        // `BaseComplexModel` fields are not used by any plugin today. Accept
        // their CLI string representation and let the plugin validate it.
        OptionKind::Complex { .. } => match value {
            Value::String(text) => Some(OptionValue::Str(text.clone())),
            _ => None,
        },
    }
}

fn validate_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::Number(number) => match number.as_i64() {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        },
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "t" | "yes" | "y" | "on" => Some(true),
            "0" | "false" | "f" | "no" | "n" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn validate_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                Some(integer)
            } else {
                // pydantic accepts integral floats such as `1.0`.
                let float = number.as_f64()?;
                if float.fract() == 0.0 && float.is_finite() {
                    Some(float as i64)
                } else {
                    None
                }
            }
        }
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn validate_float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
