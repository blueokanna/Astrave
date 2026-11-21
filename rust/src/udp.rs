use crate::core::msg::{UdpPacket, UdpSocketAddr};
use std::cell::RefCell;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::Instant;

pub const CHANNEL_CAP: usize = 1024;
const MAX_LOCAL_PEERS: usize = 1024;
pub const MAX_DATAGRAM: usize = u16::MAX as usize;
pub fn queue_capacity(packet_size: usize) -> usize {
    (2 * 1024 * 1024 / packet_size.clamp(512, MAX_DATAGRAM)).clamp(1, CHANNEL_CAP)
}
pub fn valid_packet(packet: &UdpPacket) -> bool {
    packet.content.len() <= MAX_DATAGRAM
        && packet
            .remote_addr
            .as_ref()
            .is_some_and(|a| a.ip.len() <= 45)
        && packet.local_addr.as_ref().is_none_or(|a| a.ip.len() <= 45)
}
pub const CLIENT_IDLE: Duration = Duration::from_secs(30);
pub const DATA_PING_INTERVAL: Duration = Duration::from_secs(30);
pub const SERVER_DATA_READ_DEADLINE: Duration = Duration::from_secs(60);

// The socket read and payload copy finish on this thread before the task yields.
fn with_receive_buffer<T>(size: usize, read: impl FnOnce(&mut [u8]) -> T) -> T {
    thread_local! { static RECEIVE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) }; }
    let size = size.clamp(512, MAX_DATAGRAM);
    RECEIVE.with_borrow_mut(|buf| {
        if buf.len() < size {
            buf.resize(size, 0);
        }
        read(&mut buf[..size])
    })
}

pub async fn forward_user_conn(
    udp: Arc<UdpSocket>,
    mut read_rx: mpsc::Receiver<UdpPacket>,
    send_tx: mpsc::Sender<UdpPacket>,
    buf_size: usize,
) {
    let udp_w = Arc::clone(&udp);
    let writer = async move {
        while let Some(pkt) = read_rx.recv().await {
            let Some(remote) = pkt.remote_addr.as_ref().and_then(|a| a.to_std()) else {
                continue;
            };
            if let Err(e) = udp_w.send_to(&pkt.content, remote).await {
                tracing::debug!(error = %e, "udp data→local write failed");
                break;
            }
        }
    };

    let reader = async {
        while udp.readable().await.is_ok() {
            let received: std::io::Result<bool> = with_receive_buffer(buf_size, |buf| {
                let (n, remote) = udp.try_recv_from(buf)?;
                match send_tx.try_reserve() {
                    Ok(slot) => slot.send(UdpPacket::new(buf[..n].to_vec(), Some(remote))),
                    Err(mpsc::error::TrySendError::Closed(_)) => return Ok(false),
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        tracing::trace!("udp sendCh full; drop packet")
                    }
                }
                Ok(true)
            });
            match received {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => {
                    tracing::debug!(error = %e, "udp local→data read ended");
                    break;
                }
            }
        }
    };
    tokio::select! { _=writer=>{}, _=reader=>{} }
}

pub async fn forwarder(
    local_addr: SocketAddr,
    mut read_rx: mpsc::Receiver<UdpPacket>,
    send_tx: mpsc::Sender<UdpPacket>,
    buf_size: usize,
    proxy_protocol_version: Option<String>,
) {
    let map: Arc<Mutex<HashMap<SocketAddr, Arc<UdpSocket>>>> = Arc::new(Mutex::new(HashMap::new()));
    let pp_ver = proxy_protocol_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut readers = tokio::task::JoinSet::new();
    while let Some(pkt) = read_rx.recv().await {
        while readers.try_join_next().is_some() {}
        let Some(remote) = pkt.remote_addr.as_ref().and_then(|a| a.to_std()) else {
            continue;
        };

        let existing = {
            let guard = map.lock().unwrap_or_else(|e| e.into_inner());
            guard.get(&remote).map(Arc::clone)
        };

        let (udp_conn, is_new) = match existing {
            Some(conn) => (conn, false),
            None => {
                if readers.len() >= MAX_LOCAL_PEERS {
                    continue;
                }
                let bind = if local_addr.is_ipv4() {
                    "0.0.0.0:0"
                } else {
                    "[::]:0"
                };
                let sock = match UdpSocket::bind(bind).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::debug!(error = %e, "udp bind ephemeral failed");
                        continue;
                    }
                };
                if let Err(e) = sock.connect(local_addr).await {
                    tracing::debug!(error = %e, %local_addr, "udp dial local failed");
                    continue;
                }
                let sock = Arc::new(sock);
                let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
                match guard.entry(remote) {
                    std::collections::hash_map::Entry::Occupied(e) => (Arc::clone(e.get()), false),
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(Arc::clone(&sock));
                        (sock, true)
                    }
                }
            }
        };

        let payload = if is_new {
            if let Some(ref ver) = pp_ver {
                match crate::core::net::build_proxy_protocol_header(remote, local_addr, ver) {
                    Ok(hdr) => {
                        let mut buf = Vec::with_capacity(hdr.len() + pkt.content.len());
                        buf.extend_from_slice(&hdr);
                        buf.extend_from_slice(&pkt.content);
                        buf
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "udp PROXY Protocol header build failed; send raw");
                        pkt.content
                    }
                }
            } else {
                pkt.content
            }
        } else {
            pkt.content
        };

        if let Err(e) = udp_conn.send(&payload).await {
            tracing::debug!(error = %e, "udp write to local failed");
            map.lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&remote);
            continue;
        }

        if is_new {
            let map_w = Arc::clone(&map);
            let send_tx = send_tx.clone();
            let udp_conn = Arc::clone(&udp_conn);
            readers.spawn(async move {
                writer_fn(LocalReaderCtx {
                    udp_conn,
                    remote,
                    map: map_w,
                    send_tx,
                    buf_size,
                })
                .await;
            });
        }
    }

    map.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

struct LocalReaderCtx {
    udp_conn: Arc<UdpSocket>,
    remote: SocketAddr,
    map: Arc<Mutex<HashMap<SocketAddr, Arc<UdpSocket>>>>,
    send_tx: mpsc::Sender<UdpPacket>,
    buf_size: usize,
}

async fn writer_fn(ctx: LocalReaderCtx) {
    let remote = UdpSocketAddr::from_std(ctx.remote);
    // One timer per connection, reset on activity. A `timeout` around every
    // `recv` would register and cancel a timer per datagram.
    let idle = tokio::time::sleep(CLIENT_IDLE);
    tokio::pin!(idle);

    loop {
        tokio::select! {
            ready = ctx.udp_conn.readable() => {
                if ready.is_err() { break; }
                let received = with_receive_buffer(ctx.buf_size, |buf| {
                    let n = ctx.udp_conn.try_recv(buf)?;
                    if let Ok(slot) = ctx.send_tx.try_reserve() {
                        slot.send(UdpPacket {
                            content: buf[..n].to_vec(), local_addr: None, remote_addr: Some(remote.clone()),
                        });
                    }
                    Ok::<_, std::io::Error>(())
                });
                match received {
                    Ok(()) if ctx.send_tx.is_closed() => break,
                    Ok(()) => idle.as_mut().reset(Instant::now() + CLIENT_IDLE),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(_) => break,
                }
            }
            _ = &mut idle => break,
        }
    }
    let mut map = ctx.map.lock().unwrap_or_else(|e| e.into_inner());
    if map
        .get(&ctx.remote)
        .is_some_and(|s| Arc::ptr_eq(s, &ctx.udp_conn))
    {
        map.remove(&ctx.remote);
    }
}
