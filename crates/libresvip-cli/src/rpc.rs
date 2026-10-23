//! `rpc server` — LibreSVIP gRPC service (port of `libresvip.cli.rpc.server`).
//!
//! The wire format is defined by `libresvip/res/protos/libresvip.proto`. Since
//! `protoc` is not available in this workspace, [`messages`] contains
//! hand-written prost messages and [`server`] implements the generated tonic
//! server scaffolding manually.

mod convert;
mod icons;
mod messages;
mod schema;
mod server;

/// Runs the gRPC server on `host:port` until the process is interrupted.
pub fn run_server(host: &str, port: u16) -> i32 {
    server::run_server(host, port)
}
