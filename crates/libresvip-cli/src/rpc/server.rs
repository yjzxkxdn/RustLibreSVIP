//! Manual tonic service implementation for `/LibreSVIP.Conversion/*`.
//!
//! `protoc` is unavailable, so instead of `tonic-build` output this module
//! reproduces the generated server skeleton by hand: `tower_service::Service`
//! for routing plus `tonic::server::NamedService`, with `ProstCodec` for the
//! hand-written messages.

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::task::{Context, Poll};

use libresvip_core::utils::translation::{get_translation, TranslationChain};
use libresvip_middlewares::{all_middlewares, middleware_ids};
use libresvip_plugins::{enabled_plugins, plugin_ids};
use tonic::codec::ProstCodec;
use tonic::codegen::{http, Body, BoxFuture, Service, StdError};
use tonic::server::{Grpc, NamedService};
use tonic::{Request, Response, Status};

use super::convert;
use super::icons;
use super::messages::{
    ConversionRequest, ConversionResponse, PluginInfo, PluginInfosRequest, PluginInfosResponse,
};
use super::schema::option_schema;

/// Fully-qualified service name from `libresvip.proto` (`package LibreSVIP`).
pub const SERVICE_NAME: &str = "LibreSVIP.Conversion";

/// Handler trait mirroring the generated `Conversion` service trait.
pub trait ConversionHandler: Send + Sync + 'static {
    fn plugin_infos(
        &self,
        request: Request<PluginInfosRequest>,
    ) -> BoxFuture<Response<PluginInfosResponse>, Status>;

    fn convert(
        &self,
        request: Request<ConversionRequest>,
    ) -> BoxFuture<Response<ConversionResponse>, Status>;
}

/// The stateless service implementation (`class Conversion(ConversionBase)`).
#[derive(Debug, Default, Clone, Copy)]
pub struct Conversion;

impl ConversionHandler for Conversion {
    fn plugin_infos(
        &self,
        request: Request<PluginInfosRequest>,
    ) -> BoxFuture<Response<PluginInfosResponse>, Status> {
        Box::pin(async move {
            let request = request.into_inner();
            // Schema generation is CPU-bound and touches every plugin; run it
            // on the blocking pool so a hypothetical panic becomes a gRPC error
            // instead of unwinding the connection task.
            let values = tokio::task::spawn_blocking(move || {
                let translation = translation_chain(&request.language);
                let mut values = Vec::new();
                match request.category {
                    2 => {
                        for middleware in all_middlewares() {
                            let info = middleware.info();
                            values.push(PluginInfo {
                                identifier: info.id.to_owned(),
                                name: info.name.to_owned(),
                                // Middleware plugins do not override `_version_`;
                                // yapsy reports the default `1.0.0`.
                                version: "1.0.0".to_owned(),
                                description: translation.gettext(info.description),
                                author: translation.gettext(info.author),
                                // Python reads `plugin.info.website` untranslated
                                // for middlewares (unlike svs plugins).
                                website: info.website.to_owned(),
                                json_schema: option_schema(
                                    info.id,
                                    &middleware.process_options(),
                                    &translation,
                                ),
                                file_format: String::new(),
                                suffixes: Vec::new(),
                                icon_base64: String::new(),
                            });
                        }
                    }
                    category => {
                        let output_category = category == 1;
                        for plugin in enabled_plugins() {
                            let readable = plugin.readable();
                            let writable = plugin.writable();
                            // `not issubclass(plugin, WriteOnlyConverterMixin)` for
                            // INPUT and `not issubclass(plugin, ReadOnlyConverterMixin)`
                            // for OUTPUT.
                            if (output_category && !writable) || (!output_category && !readable) {
                                continue;
                            }
                            let info = plugin.info();
                            let suffixes = if output_category {
                                vec![info.suffix().to_owned()]
                            } else {
                                info.suffixes
                                    .iter()
                                    .map(|suffix| (*suffix).to_owned())
                                    .collect()
                            };
                            let schema = if output_category {
                                plugin.output_options()
                            } else {
                                plugin.input_options()
                            };
                            values.push(PluginInfo {
                                identifier: info.id.to_owned(),
                                name: info.name.to_owned(),
                                version: plugin.version().to_owned(),
                                description: translation.gettext(info.description),
                                author: translation.gettext(info.author),
                                website: translation.gettext(info.website),
                                json_schema: option_schema(info.id, &schema, &translation),
                                file_format: translation.gettext(info.file_format),
                                suffixes,
                                icon_base64: icons::icon_base64(info.id),
                            });
                        }
                    }
                }
                values
            })
            .await
            .map_err(|error| Status::unknown(format!("plugin infos task failed: {error}")))?;
            Ok(Response::new(PluginInfosResponse { values }))
        })
    }

    fn convert(
        &self,
        request: Request<ConversionRequest>,
    ) -> BoxFuture<Response<ConversionResponse>, Status> {
        Box::pin(async move {
            let request = request.into_inner();
            // `asyncio.to_thread` in Python; blocking plugin work belongs off
            // the async executor.
            let response = tokio::task::spawn_blocking(move || convert::convert(&request))
                .await
                .map_err(|error| Status::unknown(format!("conversion task failed: {error}")))?
                .map_err(Status::unknown)?;
            Ok(Response::new(response))
        })
    }
}

fn translation_chain(language: &str) -> TranslationChain {
    let mut ids = plugin_ids();
    let middleware_ids = middleware_ids();
    ids.extend(middleware_ids.iter().copied());
    get_translation(language, &ids)
}

// ---------------------------------------------------------------------------
// Generated-server equivalent
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ConversionServer<T> {
    inner: Arc<T>,
}

impl<T> ConversionServer<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

impl<T> Clone for ConversionServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T, B> Service<http::Request<B>> for ConversionServer<T>
where
    T: ConversionHandler,
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        match req.uri().path() {
            "/LibreSVIP.Conversion/PluginInfos" => {
                struct PluginInfosSvc<T: ConversionHandler>(Arc<T>);
                impl<T: ConversionHandler> tonic::server::UnaryService<PluginInfosRequest> for PluginInfosSvc<T> {
                    type Response = PluginInfosResponse;
                    type Future = BoxFuture<Response<Self::Response>, Status>;

                    fn call(&mut self, request: Request<PluginInfosRequest>) -> Self::Future {
                        let inner = self.0.clone();
                        Box::pin(async move { inner.plugin_infos(request).await })
                    }
                }
                let inner = self.inner.clone();
                let fut = async move {
                    let method = PluginInfosSvc(inner);
                    let codec = ProstCodec::<PluginInfosResponse, PluginInfosRequest>::default();
                    let mut grpc = Grpc::new(codec);
                    let response = grpc.unary(method, req).await;
                    Ok(response)
                };
                Box::pin(fut)
            }
            "/LibreSVIP.Conversion/Convert" => {
                struct ConvertSvc<T: ConversionHandler>(Arc<T>);
                impl<T: ConversionHandler> tonic::server::UnaryService<ConversionRequest> for ConvertSvc<T> {
                    type Response = ConversionResponse;
                    type Future = BoxFuture<Response<Self::Response>, Status>;

                    fn call(&mut self, request: Request<ConversionRequest>) -> Self::Future {
                        let inner = self.0.clone();
                        Box::pin(async move { inner.convert(request).await })
                    }
                }
                let inner = self.inner.clone();
                let fut = async move {
                    let method = ConvertSvc(inner);
                    let codec = ProstCodec::<ConversionResponse, ConversionRequest>::default();
                    let mut grpc = Grpc::new(codec);
                    let response = grpc.unary(method, req).await;
                    Ok(response)
                };
                Box::pin(fut)
            }
            _ => Box::pin(async move {
                Ok(http::Response::builder()
                    .status(200)
                    .header("grpc-status", "12")
                    .header("content-type", "application/grpc")
                    .body(tonic::body::empty_body())
                    .unwrap_or_else(|_| http::Response::new(tonic::body::empty_body())))
            }),
        }
    }
}

impl<T> NamedService for ConversionServer<T> {
    const NAME: &'static str = SERVICE_NAME;
}

/// Runs the gRPC server on `host:port` until the process is interrupted.
pub fn run_server(host: &str, port: u16) -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    runtime.block_on(async move {
        let Some(address) = resolve(host, port) else {
            eprintln!("cannot resolve {host}:{port}");
            return 1;
        };
        // Bind once up-front so the "Serving on" line is only printed when the
        // address is actually available (Python's `server.start()` semantics).
        match std::net::TcpListener::bind(address) {
            Ok(listener) => drop(listener),
            Err(error) => {
                eprintln!("{error}");
                return 1;
            }
        }
        println!("Serving on {host}:{port}");
        match tonic::transport::Server::builder()
            .add_service(ConversionServer::new(Conversion))
            .serve(address)
            .await
        {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("{error}");
                1
            }
        }
    })
}

fn resolve(host: &str, port: u16) -> Option<SocketAddr> {
    (host, port).to_socket_addrs().ok()?.next()
}
