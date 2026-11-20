use crate::core::msg::{self, Codec, FrameWriter, Message};
use crate::core::transport::DynStream;
use crate::core::udp::{MAX_DATAGRAM, forwarder, queue_capacity, spawn_data_ping, valid_packet};
use anyhow::{Result, anyhow};
use std::net::SocketAddr;
use tokio::io::AsyncRead;
use tokio::sync::{mpsc, oneshot};
use tokio_util::task::AbortOnDropHandle;

/// Drain at most this many queued datagrams per write.
const WRITE_BATCH: usize = 64;

pub async fn run_udp_session(
    data: DynStream,
    local_addr: SocketAddr,
    packet_size: usize,
    proxy_protocol_version: Option<String>,
    mut cancel_rx: oneshot::Receiver<()>,
    codec: Codec,
) -> Result<()> {
    let (reader, writer) = tokio::io::split(data);

    let (to_data_tx, mut to_data_rx) = mpsc::channel::<Message>(queue_capacity(packet_size));
    let (from_data_tx, from_data_rx) = mpsc::channel(queue_capacity(MAX_DATAGRAM));

    let ping = AbortOnDropHandle::new(spawn_data_ping(to_data_tx.clone()));
    let forward = {
        let tx = to_data_tx;
        tokio::spawn(async move {
            forwarder(
                local_addr,
                from_data_rx,
                tx,
                packet_size,
                proxy_protocol_version,
            )
            .await;
        })
    };

    let (fail_tx, mut fail_rx) = mpsc::channel::<anyhow::Error>(1);
    let forward = AbortOnDropHandle::new(forward);
    let reader_task =
        AbortOnDropHandle::new(tokio::spawn(data_reader(reader, from_data_tx, fail_tx)));

    let mut frames = FrameWriter::with_capacity(writer, 8 * 1024);
    let mut batch: Vec<Message> = Vec::with_capacity(WRITE_BATCH);

    let result = loop {
        tokio::select! {
            _ = &mut cancel_rx => {
                tracing::info!(%local_addr, "udp session replaced");
                break Ok(());
            }
            err = fail_rx.recv() => {
                break match err {
                    Some(e) => Err(e),
                    None => Ok(()),
                };
            }
            queued = to_data_rx.recv_many(&mut batch, WRITE_BATCH) => {
                if queued == 0 {
                    break Ok(());
                }
                let written = frames.write_batch(&batch, codec).await;
                batch.clear();
                if let Err(e) = written {
                    break Err(anyhow!("udp data write: {e}"));
                }
            }
        }
    };

    reader_task.abort();
    ping.abort();
    forward.abort();
    let _ = reader_task.await;
    let _ = ping.await;
    let _ = forward.await;
    result
}

async fn data_reader<R: AsyncRead + Unpin + Send + 'static>(
    mut reader: R,
    from_data_tx: mpsc::Sender<crate::core::msg::UdpPacket>,
    fail_tx: mpsc::Sender<anyhow::Error>,
) {
    // One scratch buffer per connection: the decoded message owns its payload,
    // so the buffer is immediately reusable.
    let mut body = Vec::with_capacity(2 * 1024);
    loop {
        match msg::read_udp_msg_into(&mut reader, &mut body).await {
            Ok((Message::UdpPacket(pkt), _)) => {
                tracing::trace!(len = pkt.content.len(), "udp packet from data");
                if valid_packet(&pkt) {
                    let _ = from_data_tx.try_send(pkt);
                }
            }
            Ok((Message::Ping(_), _)) => {}
            Ok((other, _)) => {
                tracing::debug!(ty = other.type_byte(), "udp data unexpected message");
            }
            Err(e) => {
                let _ = fail_tx.send(anyhow!("udp data read: {e}")).await;
                return;
            }
        }
    }
}
