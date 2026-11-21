use super::gw::{
    HttpGw, HttpRoute, build_domains, expand_locations, normalize_host, route_basic_auth_ok,
};
use crate::access::prepare_ingress;
use crate::control::Control;
use anyhow::{Result, anyhow};
use astrave_desktop::core::limit::{BandwidthLimiter, maybe_limit};
use astrave_desktop::core::msg::NewTunnel;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

pub struct HttpTunnel {
    pub name: String,
    pub domains: Vec<String>,
    gw: Arc<HttpGw>,
    closed: AtomicBool,
}

impl HttpTunnel {
    pub async fn register(
        np: &NewTunnel,
        control: Arc<Control>,
        gw: Arc<HttpGw>,
        sub_domain_host: &str,
        limiter: Option<Arc<BandwidthLimiter>>,
    ) -> Result<Self> {
        let domains = build_domains(&np.domains, sub_domain_host)?;
        let name = np.tunnel_name.clone();
        let locations = expand_locations(&np.locations);
        gw.register(
            &domains,
            &locations,
            HttpRoute {
                tunnel_name: name.clone(),
                control: Arc::downgrade(&control),
                host_header_rewrite: np.host_header_rewrite.clone(),
                basic_auth_user: np.basic_auth_user.clone(),
                basic_auth_password: np.basic_auth_password.clone(),
                limiter,
            },
        )?;

        tracing::info!(
            tunnel = %name,
            domains = ?domains,
            locations = ?locations,
            basic_auth = !np.basic_auth_user.is_empty() || !np.basic_auth_password.is_empty(),
            "http tunnel registered"
        );

        Ok(Self {
            name,
            domains,
            gw,
            closed: AtomicBool::new(false),
        })
    }

    pub async fn close(&self) {
        self.shutdown();
    }

    fn shutdown(&self) {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.gw.unregister_tunnel(&self.domains, &self.name);
        }
    }
}

impl Drop for HttpTunnel {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub async fn run_http_gw_listener(
    bind_addr: String,
    port: u16,
    gw: Arc<HttpGw>,
    keepalive: astrave_desktop::core::net::TcpKeepaliveConfig,
    shutdown: Arc<Notify>,
) -> Result<()> {
    let addr = format!("{bind_addr}:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "http gateway listener ready");

    loop {
        tokio::select! {
            _ = shutdown.notified() => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) => {
                        astrave_desktop::core::net::tune_tcp_stream(&stream, keepalive);
                        let gw = Arc::clone(&gw);
                        tokio::spawn(async move {
                            if let Err(e) = handle_http_ingress(gw, stream, peer).await {
                                tracing::debug!(%peer, error = %e, "http ingress ended");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "http gateway accept failed");
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

struct ParsedHttpHead {
    host: String,
    path: String,
    is_proxy_request: bool,
    authorization: Option<String>,
    proxy_authorization: Option<String>,
}

async fn handle_http_ingress(
    gw: Arc<HttpGw>,
    stream: TcpStream,
    peer: std::net::SocketAddr,
) -> Result<()> {
    let mut ingress = prepare_ingress(stream, peer);
    let (mut raw, body_prefix, head) = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        astrave_desktop::core::net::read_request_head(&mut ingress.stream, routing_headers),
    )
    .await??;

    let Some(route) = gw.lookup(&head.host, &head.path) else {
        tracing::debug!(
            peer = %ingress.peer,
            source = %ingress.source,
            host = %head.host,
            path = %head.path,
            "http no route"
        );
        write_not_found(&mut ingress.stream).await;
        return Ok(());
    };

    if !route_basic_auth_ok(
        &route,
        head.is_proxy_request,
        head.authorization.as_deref(),
        head.proxy_authorization.as_deref(),
    ) {
        tracing::debug!(
            tunnel = %route.tunnel_name,
            peer = %ingress.peer,
            proxy_mode = head.is_proxy_request,
            "http basic auth failed"
        );
        if head.is_proxy_request {
            write_proxy_unauthorized(&mut ingress.stream).await;
        } else {
            write_unauthorized(&mut ingress.stream).await;
        }
        return Ok(());
    }

    let Some(control) = route.control.upgrade() else {
        write_not_found(&mut ingress.stream).await;
        return Err(anyhow!("http tunnel client gone: {}", route.tunnel_name));
    };

    let had_host = astrave_desktop::core::net::forward_request_headers(
        &mut raw,
        &route.host_header_rewrite,
        &ingress.source.ip().to_string(),
        "http",
    )?;

    anyhow::ensure!(
        had_host || route.host_header_rewrite.is_empty(),
        "Host header not found for rewrite"
    );

    let data = match control.get_data_conn().await {
        Ok(w) => w,
        Err(e) => {
            write_bad_gateway(&mut ingress.stream).await;
            return Err(e);
        }
    };
    let data = control
        .start_data_conn(
            data,
            &route.tunnel_name,
            ingress.source.ip().to_string(),
            ingress.source.port(),
            ingress
                .local
                .map(|a| a.ip().to_string())
                .unwrap_or_default(),
            ingress.local.map(|a| a.port()).unwrap_or(0),
        )
        .await?;

    let mut data = maybe_limit(data.stream, route.limiter.clone());
    let preamble_len = (raw.len() + body_prefix.len()) as u64;
    data.write_all(&raw).await?;
    if !body_prefix.is_empty() {
        data.write_all(&body_prefix).await?;
    }
    tracing::debug!(
        tunnel = %route.tunnel_name,
        host = %head.host,
        path = %head.path,
        peer = %ingress.peer,
        source = %ingress.source,
        "http joining ingress <-> data"
    );
    let _guard = control.metrics.track_connection(&route.tunnel_name, "http");
    let (to_data, from_data, err) =
        astrave_desktop::core::io::join_counted(ingress.stream, data).await;
    control.metrics.record_traffic(
        &route.tunnel_name,
        to_data.saturating_add(preamble_len),
        from_data,
    );
    if let Some(e) = err {
        tracing::debug!(tunnel = %route.tunnel_name, error = %e, "http join ended");
    }
    Ok(())
}

fn routing_headers(req: &httparse::Request<'_, '_>) -> Result<ParsedHttpHead> {
    let target = req.path.unwrap_or("/");
    let (is_proxy_request, path) = classify_request_target(req.method.unwrap_or(""), target);
    let host = header_value(req, "host")
        .or_else(|| host_from_absolute_target(target))
        .ok_or_else(|| anyhow!("missing Host header"))?;
    Ok(ParsedHttpHead {
        host: normalize_host(&host),
        path,
        is_proxy_request,
        authorization: header_value(req, "authorization"),
        proxy_authorization: header_value(req, "proxy-authorization"),
    })
}

fn header_value(req: &httparse::Request<'_, '_>, name: &str) -> Option<String> {
    req.headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| String::from_utf8_lossy(h.value).into_owned())
}

fn classify_request_target(method: &str, target: &str) -> (bool, String) {
    if method.eq_ignore_ascii_case("CONNECT") {
        return (true, "/".into());
    }
    if let Some(rest) = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
        .or_else(|| target.strip_prefix("HTTP://"))
        .or_else(|| target.strip_prefix("HTTPS://"))
    {
        let path = match rest.find('/') {
            Some(i) => rest[i..].to_string(),
            None => "/".into(),
        };
        return (true, path);
    }
    (false, target.to_string())
}

fn host_from_absolute_target(target: &str) -> Option<String> {
    let rest = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
        .or_else(|| target.strip_prefix("HTTP://"))
        .or_else(|| target.strip_prefix("HTTPS://"))?;
    let hostport = rest.split('/').next().unwrap_or(rest);
    if hostport.is_empty() {
        None
    } else {
        Some(hostport.to_string())
    }
}

pub async fn write_not_found<W: AsyncWrite + Unpin>(stream: &mut W) {
    let body = "Not Found\n";
    let resp = format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes()).await;
}

async fn write_unauthorized<W: AsyncWrite + Unpin>(stream: &mut W) {
    let body = "Unauthorized\n";
    let resp = format!(
        "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"Restricted\"\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes()).await;
}

async fn write_proxy_unauthorized<W: AsyncWrite + Unpin>(stream: &mut W) {
    let body = "Proxy Authentication Required\n";
    let resp = format!(
        "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"Restricted\"\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes()).await;
}

async fn write_bad_gateway<W: AsyncWrite + Unpin>(stream: &mut W) {
    let body = "Bad Gateway\n";
    let resp = format!(
        "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes()).await;
}
