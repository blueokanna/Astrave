//! Tokio stream adapters for the wire contracts in [`crate::proto`].

pub use crate::proto::auth::{AUTH_SKEW_SECS, AuthFailure};
pub use crate::proto::msg::*;
pub use crate::proto::{Codec, FrameError, FrameHeader, MAX_FRAME_BODY};

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Encodes a queued message or datagram directly into a batch.
pub trait FramePayload {
    fn encode_frame_into(&self, codec: Codec, out: &mut Vec<u8>) -> Result<(), FrameError>;
}

impl FramePayload for Message {
    fn encode_frame_into(&self, codec: Codec, out: &mut Vec<u8>) -> Result<(), FrameError> {
        Message::encode_frame_into(self, codec, out)
    }
}

impl FramePayload for UdpPacket {
    fn encode_frame_into(&self, codec: Codec, out: &mut Vec<u8>) -> Result<(), FrameError> {
        UdpPacket::encode_frame_into(self, codec, out)
    }
}

/// Reuses an encoding buffer across control frames and UDP batches.
pub struct FrameWriter<W> {
    inner: W,
    frame: Vec<u8>,
}

impl<W> FrameWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            frame: Vec::with_capacity(1024),
        }
    }

    pub fn with_capacity(inner: W, capacity: usize) -> Self {
        Self {
            inner,
            frame: Vec::with_capacity(capacity),
        }
    }

    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: AsyncWrite + Unpin> FrameWriter<W> {
    /// Writes one frame and flushes it.
    pub async fn write(&mut self, msg: &Message, codec: Codec) -> Result<(), MessageWriteError> {
        self.frame.clear();
        msg.encode_frame_into(codec, &mut self.frame)?;
        self.inner.write_all(&self.frame).await?;
        self.inner.flush().await?;
        Ok(())
    }

    /// Writes and flushes a batch, returning its encoded size.
    pub async fn write_batch<T: FramePayload>(
        &mut self,
        msgs: &[T],
        codec: Codec,
    ) -> Result<usize, MessageWriteError> {
        self.frame.clear();
        for msg in msgs {
            msg.encode_frame_into(codec, &mut self.frame)?;
        }
        if self.frame.is_empty() {
            return Ok(0);
        }
        self.inner.write_all(&self.frame).await?;
        self.inner.flush().await?;
        Ok(self.frame.len())
    }
}

#[derive(Debug, Error)]
pub enum MessageReadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Frame(#[from] FrameError),
}

#[derive(Debug, Error)]
pub enum MessageWriteError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Frame(#[from] FrameError),
}

/// Returns the frame and its codec so replies can use the peer's encoding.
pub async fn read_msg<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<(Message, Codec), MessageReadError> {
    let mut body = Vec::new();
    read_msg_into(reader, &mut body).await
}

/// The decoded message owns its payload, so the caller can reuse `body`.
pub async fn read_msg_into<R: AsyncRead + Unpin>(
    reader: &mut R,
    body: &mut Vec<u8>,
) -> Result<(Message, Codec), MessageReadError> {
    read_frame_limited(reader, body, MAX_FRAME_BODY).await
}

pub async fn read_udp_msg_into<R: AsyncRead + Unpin>(
    reader: &mut R,
    body: &mut Vec<u8>,
) -> Result<(Message, Codec), MessageReadError> {
    // A maximum-size UDP datagram in JSON uses under 88 KiB of base64.
    read_frame_limited(reader, body, 128 * 1024).await
}

async fn read_frame_limited<R: AsyncRead + Unpin>(
    reader: &mut R,
    body: &mut Vec<u8>,
    limit: u32,
) -> Result<(Message, Codec), MessageReadError> {
    let mut raw = [0u8; crate::proto::frame::HEADER_LEN];
    reader.read_exact(&mut raw).await?;
    let header = FrameHeader::decode(raw)?;
    if header.len > limit {
        return Err(FrameError::TooLarge(header.len).into());
    }

    body.clear();
    // Grow with received bytes so an idle peer cannot reserve a full frame by
    // sending only the length prefix. Take preserves the next frame boundary.
    let mut payload = reader.take(header.len.into());
    while payload.limit() > 0 {
        if payload.read_buf(body).await? == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
        }
    }

    let msg = Message::decode_body(header.type_byte, body, header.codec)?;
    Ok((msg, header.codec))
}

/// Writes one frame and flushes it.
pub async fn write_msg<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &Message,
    codec: Codec,
) -> Result<(), MessageWriteError> {
    let frame = msg.encode_frame(codec)?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn datagram_reader_limits_the_frame_before_allocation() {
        let raw = FrameHeader::new(TYPE_UDP_PACKET, Codec::Json, 256 * 1024).unwrap();
        let mut head = [0; crate::proto::frame::HEADER_LEN];
        raw.encode(&mut head);
        let mut body = Vec::new();
        assert!(matches!(
            read_udp_msg_into(&mut head.as_slice(), &mut body).await,
            Err(MessageReadError::Frame(FrameError::TooLarge(_)))
        ));
        assert_eq!(body.capacity(), 0);
    }

    #[tokio::test]
    async fn batches_frames_in_order() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let mut writer = FrameWriter::new(client);
        let msgs = vec![
            Message::Ping(Ping {
                auth_digest: "aa".into(),
                timestamp: 1,
            }),
            Message::Pong(Pong::default()),
        ];

        let written = writer.write_batch(&msgs, Codec::Binary).await.unwrap();
        assert!(written > 0);
        drop(writer);

        let (first, codec) = read_msg(&mut server).await.unwrap();
        assert_eq!(codec, Codec::Binary);
        let (second, _) = read_msg(&mut server).await.unwrap();
        assert!(matches!(first, Message::Ping(_)));
        assert!(matches!(second, Message::Pong(_)));
    }

    #[tokio::test]
    async fn read_into_reuses_the_body_buffer() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let mut writer = FrameWriter::new(client);
        for _ in 0..3 {
            writer
                .write(
                    &Message::Ping(Ping {
                        auth_digest: "bb".into(),
                        timestamp: 2,
                    }),
                    Codec::Json,
                )
                .await
                .unwrap();
        }
        drop(writer);

        let mut body = Vec::new();
        let mut capacity_after_first = 0usize;
        for i in 0..3 {
            let (msg, codec) = read_msg_into(&mut server, &mut body).await.unwrap();
            assert_eq!(codec, Codec::Json);
            assert!(matches!(msg, Message::Ping(_)));
            if i == 0 {
                capacity_after_first = body.capacity();
                assert!(capacity_after_first > 0);
            } else {
                assert_eq!(
                    body.capacity(),
                    capacity_after_first,
                    "the scratch buffer must not grow after the first frame"
                );
            }
        }
    }
}
