//! `mcp serve` — MCP server on stdio (port of `libresvip.cli.mcp.server`).
//!
//! Transport: newline-delimited JSON-RPC 2.0 on stdin/stdout, exactly like
//! `zeromcp.McpServer.stdio` (one request per line, one response per line,
//! notifications produce no response, blank lines are skipped). Responses use
//! Python's patched compact separators, so a line is structurally identical to
//! `json.dumps(response)` in the Python server.
//!
//! Protocol surface: `initialize`, `ping`, `tools/list`, `tools/call`,
//! `notifications/initialized`, `notifications/cancelled`,
//! `logging/setLevel`, `resources/list`, `resources/templates/list`,
//! `resources/read`, `prompts/list` and `prompts/get`, with the same error
//! codes (`-32700`, `-32600`, `-32601`, `-32602`, `-32000`, `-32002`) and the
//! same parameter-validation wording as `zeromcp.jsonrpc`.

mod jsonrpc;
mod options;
mod pyjson;
mod session;
mod tools;

use std::io::{self, BufRead, BufReader, Write};

use serde_json::{json, Map, Value};

use jsonrpc::{early_error, response, ParamKind, ParamSpec, RpcError};
use session::ProjectSession;
use tools::{Structured, Tool};

/// `zeromcp.MCP_PROTOCOL_VERSION`.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
/// `zeromcp.SUPPORTED_PROTOCOL_VERSIONS`.
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 4] =
    ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
/// `zeromcp._LOG_LEVELS`.
const LOG_LEVELS: [&str; 8] = [
    "debug",
    "info",
    "notice",
    "warning",
    "error",
    "critical",
    "alert",
    "emergency",
];

/// Serves the MCP protocol on stdin/stdout until EOF.
pub fn run_stdio() -> i32 {
    let mut server = Server::new();
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    let mut buffer: Vec<u8> = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let text = match std::str::from_utf8(&buffer) {
            Ok(text) => text,
            Err(_) => {
                let error = RpcError::parse_error("invalid utf-8 sequence");
                if write_message(&mut writer, &early_error(error)).is_err() {
                    return 0;
                }
                continue;
            }
        };
        // `request.strip()` in `McpServer.stdio`.
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(message) = server.handle_line(text) {
            if write_message(&mut writer, &message).is_err() {
                return 0;
            }
        }
    }
    0
}

fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    writeln!(writer, "{}", pyjson::dumps(message))?;
    writer.flush()
}

struct Server {
    tools: Vec<Tool>,
    session: ProjectSession,
    protocol_version: Option<String>,
}

impl Server {
    fn new() -> Self {
        Self {
            tools: tools::registry(),
            session: ProjectSession::default(),
            protocol_version: None,
        }
    }

    /// `self._stdio_protocol_version or MCP_PROTOCOL_VERSION`.
    fn current_protocol(&self) -> &str {
        self.protocol_version
            .as_deref()
            .unwrap_or(MCP_PROTOCOL_VERSION)
    }

    /// `_protocol_at_least` — protocol versions are ISO dates.
    fn protocol_at_least(&self, version: &str) -> bool {
        self.current_protocol() >= version
    }

    /// `_tool_validation_errors_are_execution_errors`.
    fn validation_errors_are_execution_errors(&self) -> bool {
        self.protocol_at_least("2025-11-25")
    }

    fn handle_line(&mut self, raw: &str) -> Option<Value> {
        let request: Value = match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(error) => {
                return Some(early_error(RpcError::parse_error(error.to_string())));
            }
        };
        let Value::Object(request) = request else {
            return Some(early_error(RpcError::invalid_request(
                "Invalid request: must be a JSON object",
            )));
        };
        if !matches!(request.get("jsonrpc"), Some(Value::String(version)) if version == "2.0") {
            return Some(early_error(RpcError::invalid_request(
                "Invalid request: 'jsonrpc' must be '2.0'",
            )));
        }
        let method = match request.get("method") {
            Some(Value::String(method)) => method.clone(),
            Some(_) => {
                return Some(early_error(RpcError::invalid_request(
                    "Invalid request: 'method' must be a string",
                )));
            }
            None => {
                return Some(early_error(RpcError::invalid_request(
                    "Invalid request: 'method' is required",
                )));
            }
        };
        let is_notification = !request.contains_key("id");
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let id_is_valid = match &id {
            Value::Null | Value::String(_) => true,
            Value::Number(number) => number.is_i64() || number.is_u64(),
            _ => false,
        };
        if !id_is_valid {
            return Some(early_error(RpcError::invalid_request(
                "Invalid request: 'id' must be a string, integer, or null",
            )));
        }
        let params = request.get("params");
        // A panic in plugin code must not take the server down: report it as a
        // JSON-RPC internal error instead (tool panics become tool errors).
        let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.dispatch(&method, params)
        })) {
            Ok(outcome) => outcome,
            Err(payload) => Err(RpcError::new(
                jsonrpc::INTERNAL_ERROR,
                format!("internal error: {}", jsonrpc::panic_message(&*payload)),
            )),
        };
        if is_notification {
            return None;
        }
        Some(response(id, outcome))
    }

    fn dispatch(&mut self, method: &str, params: Option<&Value>) -> Result<Value, RpcError> {
        let meta = ParamSpec::optional("_meta", ParamKind::OptionalDict);
        match method {
            "ping" => {
                jsonrpc::validate_params(params, &[meta])?;
                Ok(json!({}))
            }
            "initialize" => self.initialize(params),
            "notifications/initialized" => {
                jsonrpc::validate_params(params, &[meta])?;
                Ok(Value::Null)
            }
            "notifications/cancelled" => {
                jsonrpc::validate_params(
                    params,
                    &[
                        ParamSpec::required("requestId", ParamKind::IntOrStr),
                        ParamSpec::optional("reason", ParamKind::OptionalStr),
                        meta,
                    ],
                )?;
                Ok(Value::Null)
            }
            "logging/setLevel" => {
                let values = jsonrpc::validate_params(
                    params,
                    &[ParamSpec::required("level", ParamKind::Str), meta],
                )?;
                let Some(Some(Value::String(level))) = values.first() else {
                    return Err(RpcError::invalid_params("Missing required params"));
                };
                if !LOG_LEVELS.contains(&level.as_str()) {
                    return Err(RpcError::invalid_params(format!(
                        "Invalid params: unsupported logging level '{level}'"
                    )));
                }
                Ok(json!({}))
            }
            "tools/list" => {
                jsonrpc::validate_params(
                    params,
                    &[ParamSpec::optional("cursor", ParamKind::OptionalStr), meta],
                )?;
                let tools: Vec<Value> = self.tools.iter().map(Tool::schema).collect();
                Ok(json!({ "tools": tools }))
            }
            "tools/call" => self.tools_call(params),
            "resources/list" => {
                jsonrpc::validate_params(
                    params,
                    &[ParamSpec::optional("cursor", ParamKind::OptionalStr), meta],
                )?;
                Ok(json!({ "resources": [] }))
            }
            "resources/templates/list" => {
                jsonrpc::validate_params(
                    params,
                    &[ParamSpec::optional("cursor", ParamKind::OptionalStr), meta],
                )?;
                Ok(json!({ "resourceTemplates": [] }))
            }
            "resources/read" => {
                let values = jsonrpc::validate_params(
                    params,
                    &[ParamSpec::required("uri", ParamKind::Str), meta],
                )?;
                let uri = match values.first() {
                    Some(Some(Value::String(uri))) => uri.clone(),
                    _ => String::new(),
                };
                Err(RpcError::with_data(
                    jsonrpc::RESOURCE_NOT_FOUND,
                    "Resource not found",
                    json!({ "uri": uri }),
                ))
            }
            "prompts/list" => {
                jsonrpc::validate_params(
                    params,
                    &[ParamSpec::optional("cursor", ParamKind::OptionalStr), meta],
                )?;
                Ok(json!({ "prompts": [] }))
            }
            "prompts/get" => {
                let values = jsonrpc::validate_params(
                    params,
                    &[
                        ParamSpec::required("name", ParamKind::Str),
                        ParamSpec::optional("arguments", ParamKind::OptionalDict),
                        meta,
                    ],
                )?;
                let name = match values.first() {
                    Some(Some(Value::String(name))) => name.clone(),
                    _ => String::new(),
                };
                Err(RpcError::method_not_found(&name))
            }
            _ => Err(RpcError::method_not_found(method)),
        }
    }

    fn initialize(&mut self, params: Option<&Value>) -> Result<Value, RpcError> {
        let values = jsonrpc::validate_params(
            params,
            &[
                ParamSpec::required("protocolVersion", ParamKind::Str),
                ParamSpec::required("capabilities", ParamKind::Dict),
                ParamSpec::required("clientInfo", ParamKind::Dict),
                ParamSpec::optional("_meta", ParamKind::OptionalDict),
            ],
        )?;
        let requested = match values.first() {
            Some(Some(Value::String(version))) => version.clone(),
            _ => String::new(),
        };
        let active = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested.as_str()) {
            requested
        } else {
            MCP_PROTOCOL_VERSION.to_owned()
        };
        self.protocol_version = Some(active.clone());
        Ok(json!({
            "protocolVersion": active,
            "capabilities": {
                "tools": {},
                "resources": {"subscribe": false, "listChanged": false},
                "prompts": {},
                "logging": {},
            },
            "serverInfo": {"name": "libresvip", "version": "1.0.0"},
            "instructions": tools::INSTRUCTIONS,
        }))
    }

    fn tools_call(&mut self, params: Option<&Value>) -> Result<Value, RpcError> {
        let values = jsonrpc::validate_params(
            params,
            &[
                ParamSpec::required("name", ParamKind::Str),
                ParamSpec::optional("arguments", ParamKind::OptionalDict),
                ParamSpec::optional("_meta", ParamKind::OptionalDict),
            ],
        )?;
        let Some(Some(Value::String(name))) = values.first() else {
            return Err(RpcError::invalid_params("Missing required params"));
        };
        let arguments = values.get(1).and_then(Option::as_ref);
        let Some(index) = self.tools.iter().position(|tool| tool.name == *name) else {
            return Err(RpcError::method_not_found(name));
        };
        let tool = &self.tools[index];
        let specs: Vec<ParamSpec> = tool
            .params
            .iter()
            .map(|param| ParamSpec {
                name: param.name,
                kind: param.kind,
                has_default: param.has_default,
            })
            .collect();
        let session = &mut self.session;
        let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match jsonrpc::validate_params(arguments, &specs) {
                Ok(arguments) => tools::call(session, tool, &arguments),
                Err(error) => Err(error),
            }
        })) {
            Ok(outcome) => outcome,
            Err(payload) => Err(RpcError::tool(format!(
                "internal error: {}",
                jsonrpc::panic_message(&*payload)
            ))),
        };
        self.format_tool_response(tool, outcome)
    }

    /// `_format_tool_response`.
    fn format_tool_response(
        &self,
        tool: &Tool,
        outcome: Result<Value, RpcError>,
    ) -> Result<Value, RpcError> {
        match outcome {
            Ok(result) => {
                let mut message = Map::new();
                message.insert(
                    "content".to_owned(),
                    json!([{"type": "text", "text": tools::content_text(&result)}]),
                );
                message.insert("isError".to_owned(), Value::Bool(false));
                match tool.structured {
                    Structured::None => {}
                    Structured::Direct => {
                        message.insert("structuredContent".to_owned(), result);
                    }
                    Structured::Wrapped => {
                        message.insert("structuredContent".to_owned(), json!({ "result": result }));
                    }
                }
                Ok(Value::Object(message))
            }
            Err(error) => {
                // `-32601` is always a JSON-RPC error; `-32602` only becomes
                // tool content once validation errors are execution errors.
                if error.code == jsonrpc::METHOD_NOT_FOUND
                    || (error.code == jsonrpc::INVALID_PARAMS
                        && !self.validation_errors_are_execution_errors())
                {
                    return Err(error);
                }
                let message = if error.message.is_empty() {
                    "Unknown error".to_owned()
                } else {
                    error.message
                };
                let mut content = Map::new();
                content.insert(
                    "content".to_owned(),
                    json!([{"type": "text", "text": message}]),
                );
                content.insert("isError".to_owned(), Value::Bool(true));
                Ok(Value::Object(content))
            }
        }
    }
}
