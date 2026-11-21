use super::{HANDSHAKE, Identity, identity, pin};
use anyhow::{Context, Result, ensure};
use quinn::{Connection, Endpoint};
use serde::Deserialize;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{Semaphore, oneshot},
    time::timeout,
};

const INTRO: &[u8] = b"astrave-peer/1";
const WAIT: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    bind: SocketAddr,
    peer_sha256: [String; 2],
    tls: Identity,
}

struct Waiting {
    address: SocketAddr,
    reply: oneshot::Sender<SocketAddr>,
}
type Slots = Mutex<[Option<Waiting>; 2]>;

pub(super) async fn serve(cfg: Config) -> Result<()> {
    let allowed = [pin(&cfg.peer_sha256[0])?, pin(&cfg.peer_sha256[1])?];
    ensure!(
        allowed[0] != allowed[1],
        "peers must have distinct certificates"
    );
    let (endpoint, _) = cfg.tls.endpoint(cfg.bind, true)?;
    let slots = Arc::new(Slots::new([None, None]));
    let limit = Arc::new(Semaphore::new(32));
    tracing::info!(bind = %endpoint.local_addr()?, "peer rendezvous listening");
    while let Some(incoming) = endpoint.accept().await {
        let Ok(permit) = limit.clone().try_acquire_owned() else {
            incoming.ignore();
            continue;
        };
        // Verify source address before allocating TLS handshake state.
        if !incoming.remote_address_validated() && incoming.may_retry() {
            let _ = incoming.retry();
            continue;
        }
        let slots = slots.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let result: Result<()> = async {
                let link = timeout(HANDSHAKE, incoming).await??;
                let certificate = identity(&link)?;
                let slot = allowed
                    .iter()
                    .position(|p| *p == certificate)
                    .context("unregistered peer")?;
                introduce(&link, slot, &slots).await
            }
            .await;
            if let Err(error) = result {
                tracing::debug!(%error, "rendezvous ended");
            }
        });
    }
    Ok(())
}

async fn introduce(link: &Connection, slot: usize, slots: &Slots) -> Result<()> {
    let mut send = timeout(HANDSHAKE, async {
        let (send, mut recv) = link.accept_bi().await?;
        ensure!(
            recv.read_to_end(INTRO.len()).await? == INTRO,
            "unknown introduction protocol"
        );
        Ok::<_, anyhow::Error>(send)
    })
    .await??;
    let address = link.remote_address();
    let (tx, rx) = oneshot::channel();
    {
        let mut slots = slots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(other) = slots[1 - slot].take()
            && other.reply.send(address).is_ok()
        {
            let _ = tx.send(other.address);
        } else {
            slots[slot] = Some(Waiting { address, reply: tx });
        }
    }
    let peer = tokio::select! {
        result = timeout(WAIT, rx) => result??,
        reason = link.closed() => anyhow::bail!("{reason}"),
    };
    send.write_all(peer.to_string().as_bytes()).await?;
    send.finish()?;
    timeout(HANDSHAKE, send.stopped()).await??;
    Ok(())
}

pub(super) async fn discover(
    endpoint: &Endpoint,
    address: SocketAddr,
    name: &str,
) -> Result<SocketAddr> {
    let link = timeout(HANDSHAKE, endpoint.connect(address, name)?).await??;
    let (mut send, mut recv) = link.open_bi().await?;
    send.write_all(INTRO).await?;
    send.finish()?;
    let bytes = timeout(WAIT + HANDSHAKE, recv.read_to_end(64)).await??;
    let remote = std::str::from_utf8(&bytes)?.parse()?;
    link.close(0u32.into(), b"introduction complete");
    Ok(remote)
}
