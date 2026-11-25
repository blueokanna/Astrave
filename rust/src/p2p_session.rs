use super::{HANDSHAKE, STREAM_LIMIT};
use anyhow::{Result, ensure};
use astrave_desktop::core::{io::join, net::enable_nodelay};
use libp2p::core::muxing::{StreamMuxerBox, StreamMuxerExt, SubstreamBox};
use multistream_select::{Version, dialer_select_proto, listener_select_proto};
use std::{future::poll_fn, io, net::SocketAddr, task::Poll};
use tokio::{
    net::{TcpListener, TcpStream},
    task::{JoinError, JoinSet},
    time::timeout,
};
use tokio_util::compat::FuturesAsyncReadCompatExt;

const PROTOCOL: &str = "/astrave/tcp/1.0.0";

pub(super) fn log_result(result: Result<Result<()>, JoinError>) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::debug!(%error, "libp2p session or stream ended"),
        Err(error) => tracing::warn!(%error, "libp2p task failed"),
    }
}

enum Stream {
    Incoming(SubstreamBox),
    Outgoing(SubstreamBox),
}

pub(super) async fn forward(
    mut mux: StreamMuxerBox,
    listener: Option<&TcpListener>,
    local: SocketAddr,
) -> Result<()> {
    let mut tasks = JoinSet::new();
    let mut pending = None;
    loop {
        let capacity = tasks.len() < STREAM_LIMIT;
        let open = pending.is_some();
        let next = poll_fn(|cx| {
            while mux.poll_unpin(cx)?.is_ready() {}
            if capacity && let Poll::Ready(stream) = mux.poll_inbound_unpin(cx)? {
                return Poll::Ready(Ok(Stream::Incoming(stream)));
            }
            if open && let Poll::Ready(stream) = mux.poll_outbound_unpin(cx)? {
                return Poll::Ready(Ok(Stream::Outgoing(stream)));
            }
            Poll::<io::Result<Stream>>::Pending
        });
        tokio::select! {
            Some(result) = tasks.join_next() => log_result(result),
            socket = async { listener.expect("consumer listener").accept().await }, if listener.is_some() && capacity && !open => {
                pending = Some(socket?.0);
            }
            result = next => match result? {
                Stream::Incoming(stream) => {
                    ensure!(listener.is_none(), "unexpected stream from publisher");
                    tasks.spawn(async move {
                        let (_, stream) = timeout(HANDSHAKE, listener_select_proto(stream, [PROTOCOL])).await??;
                        let socket = timeout(HANDSHAKE, TcpStream::connect(local)).await??;
                        enable_nodelay(&socket);
                        join(socket, stream.compat()).await?;
                        Ok(())
                    });
                }
                Stream::Outgoing(stream) => {
                    let socket = pending.take().expect("outbound stream requested for socket");
                    tasks.spawn(async move {
                        enable_nodelay(&socket);
                        let (_, stream) = timeout(HANDSHAKE, dialer_select_proto(stream, [PROTOCOL], Version::V1)).await??;
                        join(socket, stream.compat()).await?;
                        Ok(())
                    });
                }
            }
        }
    }
}
