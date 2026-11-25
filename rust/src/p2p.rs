//! A fixed TCP service carried over authenticated libp2p substreams.
#[path = "p2p_session.rs"]
mod session;

use anyhow::{Context, Result, bail, ensure};
use astrave_desktop::core::{
    config::{LogConfig, parse_config_toml},
    logging,
    net::tune_tcp_stream,
};
use futures_util::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Transport,
    core::{
        Endpoint,
        muxing::StreamMuxerBox,
        transport::{Boxed, DialOpts, ListenerId, PortUse, TransportEvent},
        upgrade::Version,
    },
    identity::Keypair,
    multiaddr::Protocol,
    noise, quic, tcp, yamux,
};
use serde::Deserialize;
use std::{
    fs::File,
    io::{Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::Semaphore,
    task::JoinSet,
    time::{sleep, timeout},
};

const HANDSHAKE: Duration = Duration::from_secs(10);
const PUBLISH_TASK_LIMIT: usize = 17; // One forwarding session plus handshake capacity.
const STREAM_LIMIT: usize = 128;
type Link = Boxed<(PeerId, StreamMuxerBox)>;

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Role {
    Publish,
    Consume,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Config {
    role: Role,
    identity_file: PathBuf,
    peer_id: PeerId,
    local: SocketAddr,
    addresses: Vec<Multiaddr>,
}

pub fn keygen(path: &Path) -> Result<PeerId> {
    let key = Keypair::generate_ed25519();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&key.to_protobuf_encoding()?)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .context("create new libp2p identity file")?;
    Ok(key.public().to_peer_id())
}

impl Config {
    fn load(path: &Path) -> Result<(Self, Keypair)> {
        let cfg: Self = parse_config_toml(&std::fs::read_to_string(path)?, path)?;
        ensure!(
            cfg.local.ip().is_loopback() && cfg.local.port() != 0,
            "local must be a loopback TCP address with a nonzero port"
        );
        ensure!(
            (1..=8).contains(&cfg.addresses.len()),
            "configure one to eight libp2p addresses"
        );
        for addr in &cfg.addresses {
            let mut parts = addr.iter();
            let valid_ip = match parts.next() {
                Some(Protocol::Ip4(ip)) => !ip.is_unspecified() || cfg.role == Role::Publish,
                Some(Protocol::Ip6(ip)) => !ip.is_unspecified() || cfg.role == Role::Publish,
                _ => false,
            };
            let port = match parts.next() {
                Some(Protocol::Tcp(port)) => Some(port),
                Some(Protocol::Udp(port)) if parts.next() == Some(Protocol::QuicV1) => Some(port),
                _ => None,
            };
            ensure!(
                valid_ip
                    && port.is_some_and(|p| p != 0 || cfg.role == Role::Publish)
                    && parts.next().is_none(),
                "invalid direct libp2p address {addr}; use IP with tcp or udp/quic-v1"
            );
        }
        let key_file = path.with_file_name(&cfg.identity_file);
        let mut bytes = Vec::new();
        File::open(key_file)
            .context("open libp2p identityFile")?
            .take(4097)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 4096, "libp2p identityFile exceeds 4 KiB");
        let key = Keypair::from_protobuf_encoding(&bytes).context("invalid libp2p identityFile")?;
        ensure!(
            key.public().to_peer_id() != cfg.peer_id,
            "peerId must identify the other endpoint"
        );
        Ok((cfg, key))
    }
}

fn transport(key: &Keypair) -> Result<Link> {
    let mut mux = yamux::Config::default();
    mux.set_max_num_streams(STREAM_LIMIT);
    let tcp = tcp::tokio::Transport::new(tcp::Config::default())
        .map(|socket, _| {
            tune_tcp_stream(&socket.0, Default::default());
            socket
        })
        .upgrade(Version::V1)
        .authenticate(noise::Config::new(key)?)
        .multiplex(mux)
        .boxed();
    let mut udp = quic::Config::new(key);
    udp.max_concurrent_stream_limit = STREAM_LIMIT as u32;
    let quic = quic::tokio::Transport::new(udp).map(|(id, mux), _| (id, StreamMuxerBox::new(mux)));
    Ok(tcp
        .or_transport(quic)
        .map(|link, _| link.into_inner())
        .boxed())
}

pub async fn run(path: &Path) -> Result<()> {
    let (cfg, key) = Config::load(path)?;
    let _guard = logging::init(&LogConfig::default())?;
    let transport = transport(&key)?;
    tracing::info!(identity = %key.public().to_peer_id(), peer = %cfg.peer_id, "starting libp2p endpoint");
    match cfg.role {
        Role::Publish => publish(transport, cfg).await,
        Role::Consume => consume(transport, cfg).await,
    }
}

async fn publish(mut transport: Link, cfg: Config) -> Result<()> {
    for addr in cfg.addresses {
        transport.listen_on(ListenerId::next(), addr)?;
    }
    let active = Arc::new(Semaphore::new(1));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            Some(result) = tasks.join_next() => session::log_result(result),
            event = transport.select_next_some() => match event {
                TransportEvent::NewAddress { listen_addr, .. } => tracing::info!(%listen_addr, "libp2p listening"),
                TransportEvent::ListenerClosed { reason, .. } => { reason?; bail!("libp2p listener closed"); }
                TransportEvent::ListenerError { error, .. } => tracing::warn!(%error, "libp2p listener error"),
                TransportEvent::Incoming { upgrade, .. } if tasks.len() < PUBLISH_TASK_LIMIT => {
                    let active = active.clone();
                    tasks.spawn(async move {
                        let (id, mux) = timeout(HANDSHAKE, upgrade).await??;
                        ensure!(id == cfg.peer_id, "unexpected remote Peer ID");
                        let _permit = active.try_acquire().context("peer already connected")?;
                        session::forward(mux, None, cfg.local).await
                    });
                }
                _ => {}
            }
        }
    }
}

async fn consume(mut transport: Link, cfg: Config) -> Result<()> {
    let local = TcpListener::bind(cfg.local)
        .await
        .context("bind libp2p consumer listener")?;
    let opts = DialOpts {
        role: Endpoint::Dialer,
        port_use: PortUse::New,
    };
    let mut delay = Duration::from_secs(1);
    loop {
        for addr in &cfg.addresses {
            let result = async {
                let dial = transport.dial(addr.clone(), opts)?;
                let (id, mux) = timeout(HANDSHAKE, dial).await??;
                ensure!(id == cfg.peer_id, "unexpected remote Peer ID");
                tracing::info!(peer = %id, %addr, "libp2p connected");
                delay = Duration::from_secs(1);
                session::forward(mux, Some(&local), cfg.local).await
            }
            .await;
            if let Err(error) = result {
                tracing::warn!(%addr, %error, "libp2p connection ended");
            }
        }
        sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(15));
    }
}
