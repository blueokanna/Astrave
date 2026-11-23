//! Introduce two certificate-pinned endpoints, then carry TCP on direct QUIC streams.
#[path = "peer_rendezvous.rs"]
mod rendezvous;

use anyhow::{Context, Result, bail, ensure};
use astrave_desktop::core::{
    config::{LogConfig, parse_config_toml},
    io::join,
    logging,
    net::enable_nodelay,
    transport,
};
use courierust::courierust_crypto::sha256::sha256;
use quinn::{Connection, Endpoint, EndpointConfig};
use serde::Deserialize;
use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
    time::{sleep, timeout},
};

const HANDSHAKE: Duration = Duration::from_secs(10);
const STREAM_LIMIT: usize = 128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    cert: String,
    key: String,
    ca: String,
}

impl Identity {
    fn endpoint(&self, bind: SocketAddr, accept: bool) -> Result<(Endpoint, std::net::UdpSocket)> {
        ensure!(
            [&self.cert, &self.key, &self.ca]
                .iter()
                .all(|p| !p.trim().is_empty()),
            "cert, key and ca are required"
        );
        let mut limits = quinn::TransportConfig::default();
        limits.keep_alive_interval(Some(Duration::from_secs(5)));
        limits.max_idle_timeout(Some(Duration::from_secs(25).try_into()?));
        limits.max_concurrent_bidi_streams((STREAM_LIMIT as u32).into());
        limits.max_concurrent_uni_streams(0u32.into());
        let limits = Arc::new(limits);
        let server = if accept {
            let mut config = quinn::ServerConfig::with_crypto(Arc::new(
                transport::server_crypto_from_tls_files(&self.cert, &self.key, &self.ca)?,
            ));
            config.transport_config(limits.clone());
            Some(config)
        } else {
            None
        };
        let mut client = quinn::ClientConfig::new(Arc::new(
            transport::client_crypto_from_tls_files(&self.cert, &self.key, &self.ca)?,
        ));
        client.transport_config(limits);
        let socket = std::net::UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        let probe = socket.try_clone()?;
        let mut endpoint = Endpoint::new(
            EndpointConfig::default(),
            server,
            socket,
            quinn::default_runtime().context("QUIC runtime")?,
        )?;
        endpoint.set_default_client_config(client);
        Ok((endpoint, probe))
    }
}

fn pin(value: &str) -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    ensure!(
        astrave_desktop::proto::hex::decode_into(value.as_bytes(), &mut bytes).is_some(),
        "expected a 64-digit certificate SHA-256 fingerprint"
    );
    Ok(bytes)
}

fn identity(conn: &Connection) -> Result<[u8; 32]> {
    let chain = conn
        .peer_identity()
        .and_then(|p| {
            p.downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
                .ok()
        })
        .context("peer certificate unavailable")?;
    Ok(sha256(
        chain
            .first()
            .context("empty peer certificate chain")?
            .as_ref(),
    ))
}

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Role {
    Publish,
    Consume,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Peer {
    role: Role,
    bind: SocketAddr,
    rendezvous: SocketAddr,
    rendezvous_name: String,
    peer_name: String,
    peer_sha256: String,
    local: SocketAddr,
    tls: Identity,
}

pub async fn run(path: &str, broker: bool) -> Result<()> {
    let path = Path::new(path);
    let source = std::fs::read_to_string(path).context("read peer configuration")?;
    let _guard = logging::init(&LogConfig::default())?;
    if broker {
        return rendezvous::serve(parse_config_toml(&source, path)?).await;
    }
    let cfg: Peer = parse_config_toml(&source, path)?;
    ensure!(
        cfg.local.ip().is_loopback() && cfg.local.port() != 0,
        "local must be a loopback TCP address with a nonzero port"
    );
    let expected = pin(&cfg.peer_sha256)?;
    let publisher = cfg.role == Role::Publish;
    let (endpoint, probe) = cfg.tls.endpoint(cfg.bind, publisher)?;
    let listener = if publisher {
        None
    } else {
        Some(
            TcpListener::bind(cfg.local)
                .await
                .context("bind peer TCP listener")?,
        )
    };
    let mut delay = Duration::from_secs(1);
    loop {
        match connect(&cfg, &endpoint, &probe, &expected).await {
            Ok(link) => {
                delay = Duration::from_secs(1);
                tracing::info!(peer = %link.remote_address(), local = %cfg.local, "direct peer link ready");
                if let Err(error) = forward(&link, listener.as_ref(), cfg.local).await {
                    tracing::warn!(%error, "peer link closed");
                }
                link.close(0u32.into(), b"link closed");
            }
            Err(error) => tracing::warn!(%error, "peer connection failed"),
        }
        sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(15));
    }
}

async fn connect(
    cfg: &Peer,
    endpoint: &Endpoint,
    probe: &std::net::UdpSocket,
    expected: &[u8; 32],
) -> Result<Connection> {
    let remote = rendezvous::discover(endpoint, cfg.rendezvous, &cfg.rendezvous_name).await?;
    let link = if cfg.role == Role::Consume {
        timeout(HANDSHAKE, endpoint.connect(remote, &cfg.peer_name)?).await??
    } else {
        // Quinn owns all reads. Probes leave through the same mapped UDP port.
        timeout(HANDSHAKE, async {
            let mut interval = tokio::time::interval(Duration::from_millis(200));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(err) = probe.send_to(b"\0", remote)
                            && err.kind() != std::io::ErrorKind::WouldBlock {
                            return Err(err.into());
                        }
                    }
                    incoming = endpoint.accept() => {
                        let incoming = incoming.context("endpoint closed")?;
                        if incoming.remote_address() == remote {
                            break Ok::<_, anyhow::Error>(incoming.await?);
                        }
                        incoming.ignore();
                    }
                }
            }
        })
        .await??
    };
    if &identity(&link)? != expected {
        link.close(1u32.into(), b"unauthorized");
        bail!("peer certificate fingerprint mismatch");
    }
    Ok(link)
}

async fn forward(
    link: &Connection,
    listener: Option<&TcpListener>,
    local: SocketAddr,
) -> Result<()> {
    let mut streams = JoinSet::<Result<()>>::new();
    loop {
        tokio::select! {
            reason = link.closed() => bail!("{reason}"),
            Some(result) = streams.join_next(), if !streams.is_empty() => {
                match result {
                    Ok(Ok(())) => (),
                    Ok(Err(e)) => tracing::debug!(%e, "peer stream closed"),
                    Err(e) => tracing::warn!(%e, "peer stream task failed"),
                }
            }
            socket = async { listener.unwrap().accept().await }, if listener.is_some() && streams.len() < STREAM_LIMIT => {
                let (socket, _) = socket?;
                enable_nodelay(&socket);
                let link = link.clone();
                streams.spawn(async move {
                    let (mut send, recv) = link.open_bi().await?;
                    // Signal even for server-first protocols such as SSH.
                    send.write_all(&[1]).await?;
                    join(socket, transport::QuicBiStream::new(send, recv)).await?;
                    Ok(())
                });
            }
            stream = link.accept_bi(), if listener.is_none() && streams.len() < STREAM_LIMIT => {
                let (send, mut recv) = stream?;
                streams.spawn(async move {
                    let mut marker = [0];
                    timeout(HANDSHAKE, recv.read_exact(&mut marker)).await??;
                    ensure!(marker == [1], "unknown peer stream type");
                    let socket = timeout(HANDSHAKE, TcpStream::connect(local)).await??;
                    enable_nodelay(&socket);
                    join(socket, transport::QuicBiStream::new(send, recv)).await?;
                    Ok(())
                });
            }
        }
    }
}
