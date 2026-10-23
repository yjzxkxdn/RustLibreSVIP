//! Hand-written prost messages for `libresvip/res/protos/libresvip.proto`.
//!
//! `protoc` is not available in this workspace, so the seven messages and two
//! enums are transcribed by hand. Field numbers and types match the proto (and
//! therefore the wire format used by `libresvip.cli.rpc.libresvip_pb`).
//!
//! `map<string, string>` / `map<string, SingleConversionResult>` are declared as
//! repeated entry messages instead of `HashMap` so that the server iterates the
//! entries in request order (Python's `dict` preserves it) and emits responses
//! in insertion order.

/// `enum PluginCategory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum PluginCategory {
    Input = 0,
    Output = 1,
    Middleware = 2,
}

/// `enum ConversionMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ConversionMode {
    Direct = 0,
    Split = 1,
    Merge = 2,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginInfo {
    #[prost(string, tag = "1")]
    pub identifier: String,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(string, tag = "3")]
    pub version: String,
    #[prost(string, tag = "4")]
    pub description: String,
    #[prost(string, tag = "5")]
    pub author: String,
    #[prost(string, tag = "6")]
    pub website: String,
    #[prost(string, tag = "7")]
    pub json_schema: String,
    #[prost(string, tag = "8")]
    pub file_format: String,
    #[prost(string, repeated, tag = "9")]
    pub suffixes: Vec<String>,
    #[prost(string, tag = "10")]
    pub icon_base64: String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginInfosRequest {
    #[prost(enumeration = "PluginCategory", tag = "1")]
    pub category: i32,
    #[prost(string, tag = "2")]
    pub language: String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginInfosResponse {
    #[prost(message, repeated, tag = "1")]
    pub values: Vec<PluginInfo>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConversionGroup {
    #[prost(string, tag = "1")]
    pub group_id: String,
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub file_contents: Vec<Vec<u8>>,
}

/// One `ConversionRequest.middleware_options` map entry (wire-compatible with
/// `map<string, string>`).
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MiddlewareOptionsEntry {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConversionRequest {
    #[prost(string, tag = "1")]
    pub input_format: String,
    #[prost(string, tag = "2")]
    pub output_format: String,
    #[prost(enumeration = "ConversionMode", tag = "3")]
    pub mode: i32,
    #[prost(int32, tag = "4")]
    pub max_track_count: i32,
    #[prost(message, repeated, tag = "5")]
    pub groups: Vec<ConversionGroup>,
    #[prost(string, tag = "6")]
    pub input_options: String,
    #[prost(string, tag = "7")]
    pub output_options: String,
    #[prost(message, repeated, tag = "8")]
    pub middleware_options: Vec<MiddlewareOptionsEntry>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SingleConversionResult {
    #[prost(bool, tag = "1")]
    pub success: bool,
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub file_contents: Vec<Vec<u8>>,
    #[prost(string, tag = "3")]
    pub error_message: String,
    #[prost(string, repeated, tag = "4")]
    pub warning_messages: Vec<String>,
}

/// One `ConversionResponse.group_results` map entry (wire-compatible with
/// `map<string, SingleConversionResult>`).
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GroupResultEntry {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(message, optional, tag = "2")]
    pub value: Option<SingleConversionResult>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConversionResponse {
    #[prost(message, repeated, tag = "1")]
    pub group_results: Vec<GroupResultEntry>,
}
