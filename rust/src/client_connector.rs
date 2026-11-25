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
    identity: Option<(Arc<RustlsClientConfig>, String)>,
}

impl TlsDialOpts {
    fn from_config(cfg: &ClientConfig) -> Result<Self> {
        let tls = &cfg.transport.tls;
        let identity = if tls.enable {
            Some((
                new_client_tls_config(&tls.cert_file, &tls.key_file, &tls.trusted_ca_file)?,
                cfg.tls_server_name(),
            ))
        } else {
            None
        };
        Ok(Self { identity })
    }

    async fn maybe_wrap(&self, stream: DynStream) -> Result<DynStream> {
        match &self.identity {
            Some((cfg, name)) => client_enable_tls(stream, cfg.clone(), name).await,
            None => Ok(stream),
        }
    }
}

pub async fn build_connector(
    cfg: &ClientConfig,
    address_attempt: usize,
) -> Result<Arc<dyn Connector>> {
    let keepalive = cfg.transport.tcp_keepalive();
    match cfg.protocol()? {
        Protocol::Tcp => {
            let tls = TlsDialOpts::from_config(cfg)?;
            if cfg.transport.tcp_mux {
                let stream = dial_tcp_tls(cfg, &tls, keepalive).await?;
                tracing::info!(
                    endpoint = %cfg.server_endpoint(),
                    tls = cfg.transport.tls.enable,
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
                tls: TlsDialOpts::from_config(cfg)?,
                keepalive,
            };
            if cfg.transport.tcp_mux {
                let stream = connector.open().await?;
                tracing::info!(
                    endpoint = %cfg.server_endpoint(),
                    protocol = protocol.as_str(),
                    tls = cfg.transport.tls.enable,
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
            let addr = resolve_udp_addr(cfg, address_attempt).await?;
            let tls = TlsDialOpts::from_config(cfg)?;
            if cfg.transport.tcp_mux {
                let stream = dial_kcp_tls(addr, &tls).await?;
                tracing::info!(
                    %addr,
                    tls = cfg.transport.tls.enable,
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
            let addr = resolve_udp_addr(cfg, address_attempt).await?;
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

async fn resolve_udp_addr(cfg: &ClientConfig, attempt: usize) -> Result<SocketAddr> {
    let port = cfg.server_port()?;
    let mut addresses: Vec<_> = crate::core::net::resolve_host(&cfg.server_host()?)
        .await?
        .into_iter()
        .map(|ip| SocketAddr::new(ip, port))
        .collect();
    // Stable ordering prevents a rotating DNS answer from repeating a failed IP.
    addresses.sort_unstable();
    addresses.dedup();
    let index = attempt.checked_rem(addresses.len()).unwrap_or(0);
    crate::core::net::ordered_addresses(&addresses, true)
        .nth(index)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("no gateway addresses"))
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
