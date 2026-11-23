use crate::core::config::{ClientConfig, CloudflareAccess};
use crate::core::net::{TcpKeepaliveConfig, connect_tcp_endpoint, tune_tcp_stream};
use crate::core::transport::{
    DynStream, Protocol, QuicSession, YamuxClient, boxed_stream, client_enable_tls,
    client_websocket_upgrade, dial_kcp, new_client_tls_config,
};
use anyhow::Result;
use async_trait::async_trait;
use rustls::ClientConfig as RustlsClientConfig;
use std::net::SocketAddr;
use std::sync::Arc;

#[async_trait]
pub trait Connector: Send + Sync {
    async fn open(&self) -> Result<DynStream>;

    fn close(&self) {}
}

struct TlsDialOpts {
    enable: bool,
    cfg: Arc<RustlsClientConfig>,
    server_name: String,
}

impl TlsDialOpts {
    fn from_config(cfg: &ClientConfig) -> Result<Self> {
        let tls = &cfg.transport.tls;
        let rustls_cfg =
            new_client_tls_config(&tls.cert_file, &tls.key_file, &tls.trusted_ca_file)?;
        Ok(Self {
            enable: tls.enable,
            cfg: rustls_cfg,
            server_name: cfg.tls_server_name().to_string(),
        })
    }

    async fn maybe_wrap(&self, stream: DynStream) -> Result<DynStream> {
        if !self.enable {
            return Ok(stream);
        }
        client_enable_tls(stream, Arc::clone(&self.cfg), &self.server_name).await
    }
}

pub async fn build_connector(cfg: &ClientConfig) -> Result<Arc<dyn Connector>> {
    let tls = TlsDialOpts::from_config(cfg)?;
    let keepalive = cfg.transport.tcp_keepalive();
    match cfg.protocol()? {
        Protocol::Tcp => {
            if cfg.transport.tcp_mux {
                let stream = dial_tcp_tls(cfg, &tls, keepalive).await?;
                tracing::info!(
                    endpoint = %cfg.server_endpoint(),
                    tls = tls.enable,
                    "tcpMux: physical TCP opened, yamux client started"
                );
                Ok(Arc::new(YamuxConnector {
                    yamux: YamuxClient::start(stream),
                }))
            } else {
                Ok(Arc::new(TcpConnector {
                    endpoint: cfg.server_endpoint(),
                    port: cfg.server_port()?,
                    tls,
                    keepalive,
                }))
            }
        }
        protocol @ (Protocol::Websocket | Protocol::Wss) => {
            let edge_tls = if protocol == Protocol::Wss {
                let host = cfg.server_host()?;
                Some((
                    host.trim_matches(['[', ']']).to_owned(),
                    new_client_tls_config("", "", &cfg.transport.ws_trusted_ca_file)?,
                ))
            } else {
                None
            };
            let connector = WebsocketConnector {
                endpoint: cfg.server_endpoint(),
                port: cfg.server_port()?,
                ws_path: cfg.transport.ws_path.clone(),
                edge_tls,
                access: cfg.transport.cloudflare_access.clone(),
                header_env: cfg.transport.ws_header_env.clone(),
                tls,
                keepalive,
            };
            if cfg.transport.tcp_mux {
                let stream = connector.open().await?;
                tracing::info!(
                    endpoint = %cfg.server_endpoint(),
                    protocol = protocol.as_str(),
                    tls = connector.tls.enable,
                    "tcpMux: physical WebSocket opened, yamux client started"
                );
                Ok(Arc::new(YamuxConnector {
                    yamux: YamuxClient::start(stream),
                }))
            } else {
                Ok(Arc::new(connector))
            }
        }
        Protocol::Kcp => {
            let addr = resolve_addr(cfg).await?;
            if cfg.transport.tcp_mux {
                let stream = dial_kcp_tls(addr, &tls).await?;
                tracing::info!(
                    %addr,
                    tls = tls.enable,
                    "tcpMux: physical KCP opened, yamux client started"
                );
                Ok(Arc::new(YamuxConnector {
                    yamux: YamuxClient::start(stream),
                }))
            } else {
                Ok(Arc::new(KcpConnector { addr, tls }))
            }
        }
        Protocol::Quic => {
            let addr = resolve_addr(cfg).await?;
            let t = &cfg.transport.tls;
            let session = QuicSession::dial(
                addr,
                &cfg.tls_server_name(),
                cfg.transport.quic.keepalive(),
                cfg.transport.quic.idle_timeout(),
                cfg.transport.quic.max_incoming_streams,
                &t.cert_file,
                &t.key_file,
                &t.trusted_ca_file,
            )
            .await?;
            tracing::info!(%addr, "quic session opened");
            Ok(Arc::new(QuicConnector {
                session: Arc::new(session),
            }))
        }
    }
}

async fn dial_tcp_tls(
    cfg: &ClientConfig,
    tls: &TlsDialOpts,
    keepalive: TcpKeepaliveConfig,
) -> Result<DynStream> {
    let stream = connect_tcp_endpoint(&cfg.server, cfg.server_port()?).await?;
    tune_tcp_stream(&stream, keepalive);
    tls.maybe_wrap(boxed_stream(stream)).await
}

async fn dial_kcp_tls(addr: SocketAddr, tls: &TlsDialOpts) -> Result<DynStream> {
    let stream = dial_kcp(addr).await?;
    tls.maybe_wrap(stream).await
}

async fn resolve_addr(cfg: &ClientConfig) -> Result<SocketAddr> {
    let endpoint = cfg.server_endpoint();
    crate::core::net::resolve_endpoint(&endpoint, cfg.server_port()?).await
}

struct YamuxConnector {
    yamux: YamuxClient,
}

#[async_trait]
impl Connector for YamuxConnector {
    async fn open(&self) -> Result<DynStream> {
        self.yamux.open_stream().await
    }

    fn close(&self) {
        self.yamux.close();
    }
}

struct TcpConnector {
    endpoint: String,
    port: u16,
    tls: TlsDialOpts,
    keepalive: TcpKeepaliveConfig,
}

#[async_trait]
impl Connector for TcpConnector {
    async fn open(&self) -> Result<DynStream> {
        let stream = connect_tcp_endpoint(&self.endpoint, self.port).await?;
        tune_tcp_stream(&stream, self.keepalive);
        self.tls.maybe_wrap(boxed_stream(stream)).await
    }
}

struct WebsocketConnector {
    endpoint: String,
    port: u16,
    ws_path: String,
    edge_tls: Option<(String, Arc<RustlsClientConfig>)>,
    access: Option<CloudflareAccess>,
    header_env: std::collections::BTreeMap<String, String>,
    tls: TlsDialOpts,
    keepalive: TcpKeepaliveConfig,
}

#[async_trait]
impl Connector for WebsocketConnector {
    async fn open(&self) -> Result<DynStream> {
        let socket = connect_tcp_endpoint(&self.endpoint, self.port).await?;
        tune_tcp_stream(&socket, self.keepalive);
        let socket = boxed_stream(socket);
        let stream = if let Some((host, config)) = &self.edge_tls {
            let edge = client_enable_tls(socket, config.clone(), host)
                .await
                .map_err(|e| e.context("WSS edge handshake"))?;
            client_websocket_upgrade(
                edge,
                &self.endpoint,
                &self.ws_path,
                self.access.as_ref(),
                Some(&self.header_env),
            )
            .await?
        } else {
            client_websocket_upgrade(socket, &self.endpoint, &self.ws_path, None, None).await?
        };
        self.tls.maybe_wrap(stream).await
    }
}

struct KcpConnector {
    addr: SocketAddr,
    tls: TlsDialOpts,
}

#[async_trait]
impl Connector for KcpConnector {
    async fn open(&self) -> Result<DynStream> {
        let stream = dial_kcp(self.addr).await?;
        self.tls.maybe_wrap(stream).await
    }
}

struct QuicConnector {
    session: Arc<QuicSession>,
}

#[async_trait]
impl Connector for QuicConnector {
    async fn open(&self) -> Result<DynStream> {
        self.session.open_stream().await
    }

    fn close(&self) {
        self.session.close();
    }
}
