//! Bidirectional byte pumping and websocket handshake helpers.

use futures_util::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::error::Result;
use crate::origin::stream::Stream;

/// Pumps bytes in both directions between an origin stream and the edge
/// stream until both directions reach the end.
///
/// Mirrors cloudflared's `PipeBidirectional`: each direction closes only
/// its own destination write side when the source ends, and the other
/// direction keeps pumping until it ends as well.
#[cfg_attr(not(edge_conn), allow(dead_code))]
pub(crate) async fn pump<R, W>(origin: Stream, edge_read: R, edge_write: W) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (mut origin_read, mut origin_write) = origin.into_parts();
    let mut edge_read = edge_read;
    let mut edge_write = edge_write;
    // Poll both writes independently: awaiting a congested write inside one
    // select branch would stop the opposite direction and can deadlock peers.
    futures_util::try_join!(
        async {
            futures_util::io::copy(&mut edge_read, &mut origin_write).await?;
            origin_write.close().await
        },
        async {
            futures_util::io::copy(&mut origin_read, &mut edge_write).await?;
            edge_write.close().await
        }
    )?;
    Ok(())
}

#[cfg(all(test, edge_conn))]
mod tests {
    use super::*;
    use futures_util::io::AsyncReadExt as _;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio_util::compat::TokioAsyncReadCompatExt;

    #[tokio::test]
    async fn backpressure_in_one_direction_does_not_stop_the_other() {
        let (mut origin_peer, origin) = tokio::io::duplex(64);
        let (edge_peer, edge) = tokio::io::duplex(64);
        let (mut peer_read, mut peer_write) = tokio::io::split(edge_peer);
        let (edge_read, edge_write) = edge.compat().split();
        let task = tokio::spawn(pump(
            Stream::from_io(origin.compat()),
            edge_read,
            edge_write,
        ));
        let exchange = async {
            // The peer sends more than the proxy's buffer before reading. Only
            // an independently polled reverse copy can release the backpressure.
            let send = async {
                peer_write.write_all(&vec![1; 64 * 1024]).await.unwrap();
                peer_write.shutdown().await.unwrap();
            };
            let read_reply = async {
                let mut response = Vec::new();
                peer_read.read_to_end(&mut response).await.unwrap();
                assert_eq!(response, vec![2; 32 * 1024]);
            };
            let receive = async {
                origin_peer.write_all(&vec![2; 32 * 1024]).await.unwrap();
                origin_peer.shutdown().await.unwrap();
                let mut request = Vec::new();
                origin_peer.read_to_end(&mut request).await.unwrap();
                assert_eq!(request, vec![1; 64 * 1024]);
            };
            tokio::join!(send, read_reply, receive);
            task.await.unwrap().unwrap();
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), exchange)
            .await
            .unwrap();
    }
}

/// Computes the RFC 6455 `Sec-WebSocket-Accept` value for a challenge key.
///
/// Consumers implementing [`StreamOrigin<WebSocketResponder>`](crate::StreamOrigin) use
/// this to answer the handshake in their `connect` method.
#[cfg_attr(not(h2_any), allow(dead_code))]
pub fn websocket_accept(challenge_key: &str) -> String {
    use base64::Engine as _;
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(challenge_key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}
