//! JSON-RPC 2.0 plumbing, mirroring `zeromcp.jsonrpc`.
//!
//! The registry validates request params before calling a method; the error
//! codes and messages below reproduce `JsonRpcRegistry` exactly.

use serde_json::{json, Value};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
/// `McpToolError` (`McpRpcRegistry.map_exception`).
pub const TOOL_ERROR: i64 = -32000;
/// `resources/read` on an unknown URI.
pub const RESOURCE_NOT_FOUND: i64 = -32002;

#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: i64, message: impl Into<String>, data: Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }

    pub fn parse_error(data: impl Into<String>) -> Self {
        Self::with_data(PARSE_ERROR, "JSON parse error", Value::String(data.into()))
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(INVALID_REQUEST, message)
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }

    pub fn method_not_found(name: &str) -> Self {
        Self::new(METHOD_NOT_FOUND, format!("Method '{name}' not found"))
    }

    pub fn tool(message: impl Into<String>) -> Self {
        Self::new(TOOL_ERROR, message)
    }

    pub fn to_value(&self) -> Value {
        let mut error = serde_json::Map::new();
        error.insert("code".to_owned(), Value::from(self.code));
        error.insert("message".to_owned(), Value::String(self.message.clone()));
        if let Some(data) = &self.data {
            error.insert("data".to_owned(), data.clone());
        }
        Value::Object(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Str,
    OptionalStr,
    Dict,
    OptionalDict,
    /// `int | str` (the cancellation `requestId`).
    IntOrStr,
    /// Any JSON value (the middleware callable annotation, which Python only
    /// checks with `isinstance(..., Middleware)`).
    Any,
}

#[derive(Clone, Copy)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: ParamKind,
    pub has_default: bool,
}

impl ParamSpec {
    pub const fn required(name: &'static str, kind: ParamKind) -> Self {
        Self {
            name,
            kind,
            has_default: false,
        }
    }

    pub const fn optional(name: &'static str, kind: ParamKind) -> Self {
        Self {
            name,
            kind,
            has_default: true,
        }
    }
}

/// Python type name used by `zeromcp`'s validation messages.
fn python_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(number) => {
            if number.as_i64().is_some() || number.as_u64().is_some() {
                "int"
            } else {
                "float"
            }
        }
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// Python `list` repr of strings: `['a', 'b']`.
fn python_list(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("'{name}'")).collect();
    format!("[{}]", quoted.join(", "))
}

/// Validates `params` like `JsonRpcRegistry._call` and returns one slot per
/// spec (`None` when the argument was not supplied).
pub fn validate_params(
    params: Option<&Value>,
    specs: &[ParamSpec],
) -> Result<Vec<Option<Value>>, RpcError> {
    match params {
        None | Some(Value::Null) => {
            if specs.iter().any(|spec| !spec.has_default) {
                return Err(RpcError::invalid_params("Missing required params"));
            }
            Ok(vec![None; specs.len()])
        }
        Some(Value::Array(items)) => {
            let required = specs.iter().filter(|spec| !spec.has_default).count();
            if items.len() < required {
                return Err(RpcError::invalid_params(format!(
                    "Invalid params: expected at least {required} arguments, got {}",
                    items.len()
                )));
            }
            if items.len() > specs.len() {
                return Err(RpcError::invalid_params(format!(
                    "Invalid params: expected at most {} arguments, got {}",
                    specs.len(),
                    items.len()
                )));
            }
            let mut values = vec![None; specs.len()];
            for (index, item) in items.iter().enumerate() {
                values[index] = Some(item.clone());
            }
            let mut validated = vec![None; specs.len()];
            for (index, slot) in values.into_iter().enumerate() {
                match slot {
                    Some(value) => {
                        validated[index] = validate_one(specs[index], &value)?;
                    }
                    None => validated[index] = None,
                }
            }
            Ok(validated)
        }
        Some(Value::Object(params)) => {
            let mut missing: Vec<String> = Vec::new();
            for spec in specs {
                if !spec.has_default && !params.contains_key(spec.name) {
                    missing.push(spec.name.to_owned());
                }
            }
            if !missing.is_empty() {
                return Err(RpcError::invalid_params(format!(
                    "Invalid params: missing required parameters: {}",
                    python_list(&missing)
                )));
            }
            let mut extra: Vec<String> = Vec::new();
            for key in params.keys() {
                if !specs.iter().any(|spec| spec.name == key) {
                    extra.push(key.clone());
                }
            }
            if !extra.is_empty() {
                return Err(RpcError::invalid_params(format!(
                    "Invalid params: unexpected parameters: {}",
                    python_list(&extra)
                )));
            }
            let mut validated = Vec::with_capacity(specs.len());
            for spec in specs {
                match params.get(spec.name) {
                    Some(value) => validated.push(validate_one(*spec, value)?),
                    None => validated.push(None),
                }
            }
            Ok(validated)
        }
        Some(_) => Err(RpcError::invalid_params(
            "Invalid params: must be array or object",
        )),
    }
}

fn validate_one(spec: ParamSpec, value: &Value) -> Result<Option<Value>, RpcError> {
    if value.is_null() {
        return match spec.kind {
            ParamKind::OptionalStr | ParamKind::OptionalDict => Ok(None),
            _ => Err(RpcError::invalid_params(format!(
                "Invalid params: {} cannot be null",
                spec.name
            ))),
        };
    }
    match spec.kind {
        ParamKind::Str => match value {
            Value::String(_) => Ok(Some(value.clone())),
            other => Err(RpcError::invalid_params(format!(
                "Invalid params: {} expected str, got {}",
                spec.name,
                python_type_name(other)
            ))),
        },
        ParamKind::OptionalStr => match value {
            Value::String(_) => Ok(Some(value.clone())),
            other => Err(RpcError::invalid_params(format!(
                "Invalid params: {} union does not contain {}",
                spec.name,
                python_type_name(other)
            ))),
        },
        ParamKind::Dict => match value {
            Value::Object(_) => Ok(Some(value.clone())),
            other => Err(RpcError::invalid_params(format!(
                "Invalid params: {} expected dict, got {}",
                spec.name,
                python_type_name(other)
            ))),
        },
        ParamKind::OptionalDict => match value {
            Value::Object(_) => Ok(Some(value.clone())),
            other => Err(RpcError::invalid_params(format!(
                "Invalid params: {} union does not contain {}",
                spec.name,
                python_type_name(other)
            ))),
        },
        ParamKind::IntOrStr => match value {
            Value::String(_) => Ok(Some(value.clone())),
            Value::Number(number) if number.is_i64() || number.is_u64() => Ok(Some(value.clone())),
            other => Err(RpcError::invalid_params(format!(
                "Invalid params: {} union does not contain {}",
                spec.name,
                python_type_name(other)
            ))),
        },
        ParamKind::Any => Ok(Some(value.clone())),
    }
}

/// Builds a JSON-RPC success/error response object.
pub fn response(id: Value, result: Result<Value, RpcError>) -> Value {
    match result {
        Ok(result) => json!({"jsonrpc": "2.0", "result": result, "id": id}),
        Err(error) => json!({"jsonrpc": "2.0", "error": error.to_value(), "id": id}),
    }
}

/// A response to a request that failed before its id was known.
pub fn early_error(error: RpcError) -> Value {
    response(Value::Null, Err(error))
}

/// Extracts the message from a caught panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_owned()
    }
}
