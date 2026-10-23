//! The MCP tool surface - port of `libresvip.cli.mcp.server`'s tool registry.
//!
//! Tool names, descriptions, JSON schemas, annotations and result shapes match
//! what `zeromcp.McpServer._generate_tool_schema` emits for the Python
//! functions, including the `middleware` property the keyword-only default
//! leaks into the generated schema.

use std::path::Path;

use libresvip_core::core::warning_types::CatchWarnings;
use libresvip_core::plugin::{Middleware, SvsPlugin};
use libresvip_middlewares::all_middlewares;
use libresvip_plugins::{enabled_plugins, plugin_by_suffix};
use serde_json::{json, Map, Value};

use super::jsonrpc::{ParamKind, RpcError};
use super::options as option_validation;
use super::pyjson;
use super::session::{summarize, ProjectSession};

pub const INSTRUCTIONS: &str = "Load a singing-synthesis project into a session handle, \
inspect a short summary, run one middleware at a time on that handle, then save it to \
another format. Never paste project contents into the conversation: pass local file paths \
and handles.";

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum Action {
    ListFormats,
    LoadProject,
    ProjectSummary,
    SaveProject,
    CloseProject,
    Middleware(&'static dyn Middleware),
}

/// How a successful result is mirrored into `structuredContent`.
#[derive(Clone, Copy, PartialEq)]
pub enum Structured {
    /// Text-only result (`save_project`, whose return type is `str`).
    None,
    /// The result object itself.
    Direct,
    /// `{"result": <value>}` for non-object return types (`close_project`).
    Wrapped,
}

pub struct ToolParam {
    pub name: &'static str,
    pub schema: Value,
    pub kind: ParamKind,
    pub has_default: bool,
    pub default_json: Option<Value>,
}

pub struct Tool {
    pub name: String,
    pub description: String,
    /// `(hint name, value)` in the order Python inserts them.
    pub annotations: Vec<(&'static str, bool)>,
    pub params: Vec<ToolParam>,
    pub output_schema: Option<Value>,
    pub structured: Structured,
    pub action: Action,
}

impl Tool {
    /// `_generate_tool_schema`.
    pub fn schema(&self) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for param in &self.params {
            let mut schema = param.schema.clone();
            if let Some(default) = &param.default_json {
                if let Value::Object(map) = &mut schema {
                    map.insert("default".to_owned(), default.clone());
                }
            }
            properties.insert(param.name.to_owned(), schema);
            if !param.has_default {
                required.push(Value::String(param.name.to_owned()));
            }
        }
        let mut annotations = Map::new();
        for (hint, flag) in &self.annotations {
            annotations.insert((*hint).to_owned(), Value::Bool(*flag));
        }
        let mut tool = Map::new();
        tool.insert("name".to_owned(), Value::String(self.name.clone()));
        tool.insert(
            "description".to_owned(),
            Value::String(self.description.clone()),
        );
        tool.insert(
            "inputSchema".to_owned(),
            json!({
                "type": "object",
                "properties": properties,
                "required": required,
            }),
        );
        if !annotations.is_empty() {
            tool.insert("annotations".to_owned(), Value::Object(annotations));
        }
        if let Some(output) = &self.output_schema {
            tool.insert("outputSchema".to_owned(), output.clone());
        }
        Value::Object(tool)
    }
}

fn summary_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "handle": {"type": "string"},
            "track_count": {"type": "integer"},
            "singing_track_count": {"type": "integer"},
            "note_count": {"type": "integer"},
            "tempo_count": {"type": "integer"},
            "time_signature_count": {"type": "integer"},
            "first_bpm": {"anyOf": [{"type": "number"}, {"type": "null"}]},
            "tracks": {"type": "array", "items": {"type": "string"}},
            "warnings": {"type": "array", "items": {"type": "string"}},
        },
        "required": [
            "handle",
            "track_count",
            "singing_track_count",
            "note_count",
            "tempo_count",
            "time_signature_count",
            "first_bpm",
            "tracks",
            "warnings",
        ],
        "additionalProperties": false,
    })
}

fn list_formats_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": {
            "type": "array",
            "items": {
                "type": "object",
                "additionalProperties": {
                    "anyOf": [
                        {"type": "string"},
                        {"type": "array", "items": {"type": "string"}},
                    ],
                },
            },
        },
    })
}

fn close_project_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"result": {"type": "boolean"}},
        "required": ["result"],
    })
}

fn handle_param(description: &'static str) -> ToolParam {
    ToolParam {
        name: "handle",
        schema: json!({"type": "string", "description": description}),
        kind: ParamKind::Str,
        has_default: false,
        default_json: None,
    }
}

fn options_param(description: &'static str) -> ToolParam {
    ToolParam {
        name: "options",
        schema: json!({
            "anyOf": [{"type": "string"}, {"type": "null"}],
            "description": description,
        }),
        kind: ParamKind::OptionalStr,
        has_default: true,
        default_json: Some(Value::Null),
    }
}

/// Builds the tool registry in registration order: the five session tools,
/// then one `apply_<identifier>` tool per middleware.
pub fn registry() -> Vec<Tool> {
    let mut tools = Vec::new();

    tools.push(Tool {
        name: "list_formats".to_owned(),
        description: "List readable and writable singing-synthesis formats.".to_owned(),
        annotations: vec![
            ("readOnlyHint", true),
            ("destructiveHint", false),
            ("idempotentHint", true),
            ("openWorldHint", false),
        ],
        params: Vec::new(),
        output_schema: Some(list_formats_schema()),
        structured: Structured::Direct,
        action: Action::ListFormats,
    });

    tools.push(Tool {
        name: "load_project".to_owned(),
        description: "Load a project file and return a session handle plus a short summary."
            .to_owned(),
        annotations: vec![
            ("destructiveHint", false),
            ("idempotentHint", false),
            ("openWorldHint", false),
        ],
        params: vec![
            ToolParam {
                name: "path",
                schema: json!({
                    "type": "string",
                    "description": "Local path of the project file to load",
                }),
                kind: ParamKind::Str,
                has_default: false,
                default_json: None,
            },
            options_param("JSON object of the format plugin's input options"),
        ],
        output_schema: Some(summary_schema()),
        structured: Structured::Direct,
        action: Action::LoadProject,
    });

    tools.push(Tool {
        name: "project_summary".to_owned(),
        description: "Return the current summary of a loaded project.".to_owned(),
        annotations: vec![
            ("readOnlyHint", true),
            ("destructiveHint", false),
            ("idempotentHint", true),
            ("openWorldHint", false),
        ],
        params: vec![handle_param("Session handle returned by load_project")],
        output_schema: Some(summary_schema()),
        structured: Structured::Direct,
        action: Action::ProjectSummary,
    });

    tools.push(Tool {
        name: "save_project".to_owned(),
        description: "Write a loaded project to a local file. Returns the written path."
            .to_owned(),
        annotations: vec![
            ("destructiveHint", true),
            ("idempotentHint", false),
            ("openWorldHint", false),
        ],
        params: vec![
            handle_param("Session handle returned by load_project"),
            ToolParam {
                name: "path",
                schema: json!({
                    "type": "string",
                    "description": "Local path to write. Its suffix selects the output format",
                }),
                kind: ParamKind::Str,
                has_default: false,
                default_json: None,
            },
            options_param("JSON object of the format plugin's output options"),
        ],
        output_schema: None,
        structured: Structured::None,
        action: Action::SaveProject,
    });

    tools.push(Tool {
        name: "close_project".to_owned(),
        description: "Drop a loaded project from the session.".to_owned(),
        annotations: vec![
            ("destructiveHint", false),
            ("idempotentHint", true),
            ("openWorldHint", false),
        ],
        params: vec![handle_param("Session handle to drop")],
        output_schema: Some(close_project_schema()),
        structured: Structured::Wrapped,
        action: Action::CloseProject,
    });

    for middleware in all_middlewares() {
        let info = middleware.info();
        let description = format!("{}. {}", info.name, info.description)
            .trim()
            .to_owned();
        tools.push(Tool {
            name: format!("apply_{}", info.id),
            description,
            annotations: vec![
                ("destructiveHint", true),
                ("idempotentHint", false),
                ("openWorldHint", false),
            ],
            params: vec![
                handle_param("Session handle returned by load_project"),
                options_param("JSON object of this middleware's process options"),
                ToolParam {
                    name: "middleware",
                    schema: json!({"type": "object"}),
                    kind: ParamKind::Any,
                    has_default: true,
                    default_json: None,
                },
            ],
            output_schema: Some(summary_schema()),
            structured: Structured::Direct,
            action: Action::Middleware(middleware),
        });
    }

    tools
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

fn argument<'a>(tool: &Tool, arguments: &'a [Option<Value>], name: &str) -> Option<&'a Value> {
    let index = tool.params.iter().position(|param| param.name == name)?;
    arguments.get(index).and_then(Option::as_ref)
}

fn argument_str<'a>(tool: &Tool, arguments: &'a [Option<Value>], name: &str) -> Option<&'a str> {
    match argument(tool, arguments, name) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

/// `Path(path).suffix` (`pathlib` returns the last dotted suffix, or `""`).
pub fn python_suffix(path: &Path) -> String {
    let Some(name) = path.file_name() else {
        return String::new();
    };
    let name = name.to_string_lossy();
    match name.rfind('.') {
        Some(index) if index > 0 && index + 1 < name.len() => name[index..].to_owned(),
        _ => String::new(),
    }
}

/// `str(pathlib.Path(text))` - pathlib normalises separators and drops `.`
/// components, which `save_project` exposes in its result.
pub fn python_path_display(text: &str) -> String {
    if text.is_empty() {
        return ".".to_owned();
    }
    let windows = cfg!(windows);
    let separator = if windows { '\\' } else { '/' };
    let mut prefix = String::new();
    let mut rest = text;
    if windows {
        let characters: Vec<char> = rest.chars().collect();
        if characters.len() >= 2 && characters[0].is_ascii_alphabetic() && characters[1] == ':' {
            prefix.push(characters[0]);
            prefix.push(':');
            rest = &rest[2..];
        }
    }
    let is_separator = |character: char| character == '/' || (windows && character == '\\');
    let rooted = rest.chars().next().is_some_and(is_separator);
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split(is_separator) {
        if part.is_empty() || part == "." {
            continue;
        }
        parts.push(part);
    }
    let mut rendered = String::new();
    rendered.push_str(&prefix);
    if rooted {
        rendered.push(separator);
    }
    rendered.push_str(&parts.join(&separator.to_string()));
    if rendered.is_empty() {
        ".".to_owned()
    } else {
        rendered
    }
}

/// `_plugin` - resolve a suffix (with its leading dot) and honour the
/// read-only / write-only mixins.
fn plugin_for(suffix: &str, writing: bool) -> Result<&'static dyn SvsPlugin, RpcError> {
    let normalized = suffix.trim_start_matches('.').to_lowercase();
    let Some(plugin) = plugin_by_suffix(&normalized) else {
        return Err(RpcError::tool(format!("Unsupported format: {suffix}")));
    };
    if writing && !plugin.writable() {
        return Err(RpcError::tool(format!("{suffix} cannot be written")));
    }
    if !writing && !plugin.readable() {
        return Err(RpcError::tool(format!("{suffix} cannot be read")));
    }
    Ok(plugin)
}

fn missing_required(name: &str) -> RpcError {
    RpcError::invalid_params(format!("Invalid params: missing required parameters: ['{name}']"))
}

fn unknown_handle(handle: &str) -> RpcError {
    RpcError::tool(format!("Unknown project handle: {handle}"))
}

/// `_warnings`: `caught.output.splitlines()` minus empty lines.
fn warning_lines(messages: Vec<String>) -> Vec<String> {
    messages
        .join("\n")
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn call(
    session: &mut ProjectSession,
    tool: &Tool,
    arguments: &[Option<Value>],
) -> Result<Value, RpcError> {
    match tool.action {
        Action::ListFormats => {
            let mut readable = Vec::new();
            let mut writable = Vec::new();
            for plugin in enabled_plugins() {
                let info = plugin.info();
                let entry = json!({
                    "id": info.id,
                    "name": info.name,
                    "suffixes": info.suffixes,
                });
                if plugin.readable() {
                    readable.push(entry.clone());
                }
                if plugin.writable() {
                    writable.push(entry);
                }
            }
            Ok(json!({"readable": readable, "writable": writable}))
        }
        Action::LoadProject => {
            let Some(path_text) = argument_str(tool, arguments, "path") else {
                return Err(missing_required("path"));
            };
            let file_path = Path::new(path_text);
            if !file_path.is_file() {
                return Err(RpcError::tool(format!("File not found: {path_text}")));
            }
            let plugin = plugin_for(&python_suffix(file_path), false)?;
            let options_text = argument_str(tool, arguments, "options");
            let parsed = option_validation::parse(
                &plugin.input_options(),
                options_text,
                "InputOptions",
            )
            .map_err(RpcError::tool)?;
            let guard = CatchWarnings::start();
            let loaded = plugin.load(file_path, &parsed);
            let warnings = warning_lines(guard.finish());
            let project = loaded.map_err(|error| RpcError::tool(error.to_string()))?;
            let handle = session.put(project);
            match session.get(&handle) {
                Some(project) => Ok(summarize(&handle, project, &warnings)),
                None => Err(RpcError::tool(format!("Unknown project handle: {handle}"))),
            }
        }
        Action::ProjectSummary => {
            let Some(handle) = argument_str(tool, arguments, "handle") else {
                return Err(missing_required("handle"));
            };
            match session.get(handle) {
                Some(project) => Ok(summarize(handle, project, &[])),
                None => Err(unknown_handle(handle)),
            }
        }
        Action::SaveProject => {
            let Some(handle) = argument_str(tool, arguments, "handle") else {
                return Err(missing_required("handle"));
            };
            let Some(path_text) = argument_str(tool, arguments, "path") else {
                return Err(missing_required("path"));
            };
            let file_path = Path::new(path_text);
            let plugin = plugin_for(&python_suffix(file_path), true)?;
            let Some(project) = session.get(handle) else {
                return Err(unknown_handle(handle));
            };
            let options_text = argument_str(tool, arguments, "options");
            let parsed = option_validation::parse(
                &plugin.output_options(),
                options_text,
                "OutputOptions",
            )
            .map_err(RpcError::tool)?;
            let guard = CatchWarnings::start();
            let dumped = plugin.dump(file_path, project, &parsed);
            let _warnings = guard.finish();
            dumped.map_err(|error| RpcError::tool(error.to_string()))?;
            Ok(Value::String(python_path_display(path_text)))
        }
        Action::CloseProject => {
            let Some(handle) = argument_str(tool, arguments, "handle") else {
                return Err(missing_required("handle"));
            };
            if !session.drop_handle(handle) {
                return Err(unknown_handle(handle));
            }
            Ok(Value::Bool(true))
        }
        Action::Middleware(middleware) => {
            let Some(handle) = argument_str(tool, arguments, "handle") else {
                return Err(missing_required("handle"));
            };
            if session.get(handle).is_none() {
                return Err(unknown_handle(handle));
            }
            let options_text = argument_str(tool, arguments, "options");
            let parsed = option_validation::parse(
                &middleware.process_options(),
                options_text,
                "ProcessOptions",
            )
            .map_err(RpcError::tool)?;
            let Some(project) = session.take(handle) else {
                return Err(unknown_handle(handle));
            };
            // Python keeps the session entry when `process` raises; keep an
            // untouched copy so the handle stays usable after a failure.
            let backup = project.clone();
            let guard = CatchWarnings::start();
            let processed = middleware.process(project, &parsed);
            let warnings = warning_lines(guard.finish());
            match processed {
                Ok(project) => {
                    session.replace(handle, project);
                    match session.get(handle) {
                        Some(project) => Ok(summarize(handle, project, &warnings)),
                        None => Err(unknown_handle(handle)),
                    }
                }
                Err(error) => {
                    session.replace(handle, backup);
                    Err(RpcError::tool(error.to_string()))
                }
            }
        }
    }
}

/// Renders `json.dumps(result, indent=2)` for non-string results, exactly like
/// `zeromcp._format_tool_response`.
pub fn content_text(result: &Value) -> String {
    match result {
        Value::String(text) => text.clone(),
        other => pyjson::dumps_pretty(other),
    }
}

