use super::routes::{Route, Routes, Target};
use anyhow::{Context, Result, ensure};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use http::{HeaderMap, Method, StatusCode, Uri, header};
use http_body_util::{BodyExt, StreamBody};
use hyper_util::rt::TokioIo;
use libcfd::{
    Body, HttpOrigin, HttpResponder, Request, Response, StreamOrigin, WebSocketConnection,
    WebSocketResponder,
};
use std::{
    future::Future,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::{
    compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt},
    io::{ReaderStream, StreamReader},
    sync::CancellationToken,
    task::TaskTracker,
};

#[derive(Clone)]
pub struct Proxy {
    pub routes: Arc<RwLock<Routes>>,
    pub cancel: CancellationToken,
    pub tasks: TaskTracker,
}

impl Proxy {
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        let cancel = self.cancel.clone();
        self.tasks.spawn(async move {
            tokio::select! { _ = cancel.cancelled() => {}, _ = future => {} }
        });
    }

    fn route(&self, request: &Request) -> Result<Arc<Route>> {
        self.routes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .find(request)
            .context("Cloudflare ingress configuration has not been received")
    }

    async fn connect(
        &self,
        route: &Route,
        uri: &Uri,
        request_host: Option<&str>,
    ) -> Result<crate::core::transport::DynStream> {
        let host = uri
            .host()
            .context("Missing origin host")?
            .trim_matches(['[', ']']);
        let port = uri.port_u16().unwrap_or(match uri.scheme_str() {
            Some("https") => 443,
            Some("ssh") => 22,
            Some("rdp") => 3389,
            Some("smb") => 445,
            _ => 80,
        });
        let tcp = tokio::time::timeout(
            route.connect_timeout,
            dial_origin(host, port, route.no_happy_eyeballs),
        )
        .await
        .context("Origin connection timed out")??;
        tcp.set_nodelay(true)?;
        socket2::SockRef::from(&tcp)
            .set_tcp_keepalive(&socket2::TcpKeepalive::new().with_time(route.tcp_keep_alive))?;
        let stream = crate::core::transport::boxed_stream(tcp);
        if let Some(tls) = &route.tls {
            let name = if route.match_sni_to_host {
                request_host.unwrap_or(host)
            } else {
                route.server_name.as_deref().unwrap_or(host)
            };
            Ok(tokio::time::timeout(
                route.tls_timeout,
                crate::core::transport::client_enable_tls(stream, tls.clone(), name),
            )
            .await
            .context("Origin TLS handshake timed out")??)
        } else {
            Ok(stream)
        }
    }

    async fn exchange(
        &self,
        mut request: Request,
        route: &Route,
        uri: &Uri,
        websocket: bool,
    ) -> Result<hyper::Response<hyper::body::Incoming>> {
        let public_name = request
            .headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<http::uri::Authority>().ok())
            .map(|a| a.host().trim_matches(['[', ']']).to_owned())
            .or_else(|| request.uri.host().map(str::to_owned));
        let stream = self.connect(route, uri, public_name.as_deref()).await?;
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        self.spawn(async move {
            if let Err(e) = connection.with_upgrades().await {
                tracing::debug!(error = %e, "Cloudflare origin HTTP connection ended");
            }
        });
        let target = request
            .uri
            .path_and_query()
            .map_or("/", |p| p.as_str())
            .to_owned();
        let public_host = request.uri.authority().map(|a| a.as_str().to_owned());
        strip_hop_headers(&mut request.headers);
        // Edge transport metadata must not be passed on as origin headers.
        request
            .headers
            .remove("cf-cloudflared-proxy-connection-upgrade");
        request.headers.remove("cf-cloudflared-proxy-src");
        if let Some(host) = &route.host_header {
            request.headers.insert(header::HOST, host.clone());
        } else if !request.headers.contains_key(header::HOST) {
            request.headers.insert(
                header::HOST,
                public_host
                    .as_deref()
                    .unwrap_or(uri.authority().unwrap().as_str())
                    .parse()?,
            );
        }
        if websocket {
            request
                .headers
                .insert(header::CONNECTION, "Upgrade".parse()?);
            request
                .headers
                .insert(header::UPGRADE, "websocket".parse()?);
        }
        let body: http_body_util::combinators::UnsyncBoxBody<Bytes, std::io::Error> =
            if request.body.size_hint() == Some(0) {
                http_body_util::Empty::new()
                    .map_err(|never| match never {})
                    .boxed_unsync()
            } else {
                let stream =
                    ReaderStream::new(request.body.compat()).map_ok(hyper::body::Frame::data);
                StreamBody::new(stream).boxed_unsync()
            };
        let mut origin = http::Request::builder()
            .method(request.method)
            .uri(target)
            .body(body)?;
        *origin.headers_mut() = request.headers;
        tokio::time::timeout(Duration::from_secs(90), sender.send_request(origin))
            .await
            .context("Origin response headers timed out")?
            .context("Origin HTTP request failed")
    }

    pub async fn http(&self, request: Request) -> Result<Response> {
        let route = self.route(&request)?;
        match &route.target {
            Target::Status(status) => Ok(Response::new(*status, HeaderMap::new(), Body::empty())),
            Target::Http(uri) => {
                let head = request.method == Method::HEAD;
                let response = self.exchange(request, &route, uri, false).await?;
                let (mut parts, body) = response.into_parts();
                strip_hop_headers(&mut parts.headers);
                let body = if head {
                    Body::empty()
                } else {
                    Body::from_reader(
                        StreamReader::new(body.into_data_stream().map_err(std::io::Error::other))
                            .compat(),
                    )
                };
                Ok(Response::new(parts.status, parts.headers, body))
            }
            Target::Tcp(_) => Ok(Response::new(
                StatusCode::BAD_REQUEST,
                HeaderMap::new(),
                Body::from_bytes(b"This route requires a WebSocket TCP client".to_vec()),
            )),
        }
    }

    pub async fn websocket(
        &self,
        request: Request,
    ) -> Result<Result<WebSocketConnection, Response>> {
        let route = self.route(&request)?;
        match &route.target {
            Target::Http(uri) => {
                let expected = request
                    .headers
                    .get("sec-websocket-key")
                    .and_then(|h| h.to_str().ok())
                    .map(libcfd::websocket_accept);
                let mut response = self.exchange(request, &route, uri, true).await?;
                if response.status() != StatusCode::SWITCHING_PROTOCOLS {
                    let (mut parts, body) = response.into_parts();
                    strip_hop_headers(&mut parts.headers);
                    return Ok(Err(Response::new(
                        parts.status,
                        parts.headers,
                        Body::from_reader(
                            StreamReader::new(
                                body.into_data_stream().map_err(std::io::Error::other),
                            )
                            .compat(),
                        ),
                    )));
                }
                ensure!(
                    expected.as_deref().is_some_and(|v| response
                        .headers()
                        .get("sec-websocket-accept")
                        .is_some_and(|h| h == v)),
                    "Invalid origin WebSocket handshake"
                );
                let headers = response.headers().clone();
                let upgraded = hyper::upgrade::on(&mut response).await?;
                Ok(Ok(WebSocketConnection {
                    response: Response::new(
                        StatusCode::SWITCHING_PROTOCOLS,
                        headers,
                        Body::empty(),
                    ),
                    origin: libcfd::Stream::from_io(TokioIo::new(upgraded).compat()),
                }))
            }
            Target::Tcp(uri) => {
                use base64::{Engine, engine::general_purpose::STANDARD};
                let mut nonce = [0u8; 16];
                let Some(key) = request
                    .headers
                    .get("sec-websocket-key")
                    .and_then(|value| value.to_str().ok())
                    .filter(|key| {
                        key.len() == 24 && STANDARD.decode_slice(key, &mut nonce).ok() == Some(16)
                    })
                else {
                    return Ok(Err(Response::new(
                        StatusCode::BAD_REQUEST,
                        HeaderMap::new(),
                        Body::empty(),
                    )));
                };
                let tcp = self.connect(&route, uri, None).await?;
                let mut headers = HeaderMap::new();
                headers.insert(header::CONNECTION, "Upgrade".parse()?);
                headers.insert(header::UPGRADE, "websocket".parse()?);
                headers.insert(
                    "sec-websocket-accept",
                    libcfd::websocket_accept(key).parse()?,
                );
                let (edge, bridge) = tokio::io::duplex(64 * 1024);
                self.spawn(async move {
                    if let Err(e) = websocket_tcp(bridge, tcp).await {
                        tracing::debug!(error = %e, "Cloudflare TCP WebSocket ended");
                    }
                });
                Ok(Ok(WebSocketConnection {
                    response: Response::new(
                        StatusCode::SWITCHING_PROTOCOLS,
                        headers,
                        Body::empty(),
                    ),
                    origin: libcfd::Stream::from_io(edge.compat()),
                }))
            }
            Target::Status(status) => {
                Ok(Err(Response::new(*status, HeaderMap::new(), Body::empty())))
            }
        }
    }
}

async fn dial_origin(
    host: &str,
    port: u16,
    sequential: bool,
) -> std::io::Result<tokio::net::TcpStream> {
    if sequential {
        return tokio::net::TcpStream::connect((host, port)).await;
    }
    let addresses: Vec<_> = tokio::net::lookup_host((host, port)).await?.collect();
    let prefer_v4 = addresses.first().is_some_and(std::net::SocketAddr::is_ipv4);
    crate::core::net::connect_tcp_addresses(&addresses, prefer_v4).await
}

impl HttpOrigin for Proxy {
    fn handle(&self, request: Request, respond: HttpResponder) {
        let proxy = self.clone();
        self.spawn(async move {
            match proxy.http(request).await {
                Ok(response) => respond.send(response),
                Err(error) => {
                    tracing::warn!(error = %error, "Cloudflare origin request failed");
                    respond.fail("Astrave could not reach the configured origin");
                }
            }
        });
    }
}

impl StreamOrigin<WebSocketResponder> for Proxy {
    fn connect(&self, request: Request, respond: WebSocketResponder) {
        let proxy = self.clone();
        self.spawn(async move {
            match proxy.websocket(request).await {
                Ok(Ok(connection)) => respond.upgrade(connection),
                Ok(Err(response)) => respond.send(response),
                Err(error) => {
                    tracing::warn!(error = %error, "Cloudflare WebSocket origin failed");
                    respond.fail("Astrave could not establish the origin stream");
                }
            }
        });
    }
}

fn strip_hop_headers(headers: &mut HeaderMap) {
    let names: Vec<_> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(','))
        .filter_map(|s| s.trim().parse::<header::HeaderName>().ok())
        .collect();
    for name in names {
        headers.remove(name);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

async fn websocket_tcp(
    edge: tokio::io::DuplexStream,
    tcp: crate::core::transport::DynStream,
) -> Result<()> {
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::{
            Message,
            protocol::{Role, WebSocketConfig},
        },
    };
    const CHUNK_SIZE: usize = 32 * 1024;
    let config = WebSocketConfig::default()
        .read_buffer_size(CHUNK_SIZE)
        .write_buffer_size(CHUNK_SIZE)
        .max_write_buffer_size(4 * CHUNK_SIZE);
    let ws = WebSocketStream::from_raw_socket(edge, Role::Server, Some(config)).await;
    let (mut sink, mut source) = ws.split();
    let (mut reader, mut writer) = tokio::io::split(tcp);
    let (flush, mut flushes) = tokio::sync::mpsc::channel(1);
    let result = {
        let upload = async {
            while let Some(message) = source.next().await {
                match message? {
                    Message::Binary(bytes) => writer.write_all(&bytes).await?,
                    Message::Text(text) => writer.write_all(text.as_bytes()).await?,
                    Message::Ping(_) => {
                        let _ = flush.try_send(());
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            Ok::<(), anyhow::Error>(())
        };
        let download = async {
            let mut buffer = vec![0; CHUNK_SIZE];
            loop {
                tokio::select! {
                    _ = flushes.recv() => sink.flush().await?,
                    count = reader.read(&mut buffer) => {
                        let count = count?;
                        if count == 0 { return Ok::<(), anyhow::Error>(()); }
                        sink.send(Message::Binary(Bytes::copy_from_slice(&buffer[..count]))).await?;
                    }
                }
            }
        };
        tokio::select! { result = upload => result, result = download => result }
    };
    let _ = sink.close().await;
    let _ = writer.shutdown().await;
    result
}
