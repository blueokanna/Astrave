//! RFC 6455 transport.
//!
//! The frame codec and the handshake primitives come from `courierust_ws`
//! (pure `no_std`, no I/O); the socket stays on tokio so a tunnel keeps one task
//! per direction instead of a thread per connection.
//!
//! The stream is a *byte* tunnel: binary fragments are concatenated on read and
//! every write becomes one binary frame. Astrave layers TLS and yamux on top, so
//! the WebSocket carries opaque bytes and never inspects them.

use super::stream::{DynStream, boxed_stream};
use crate::core::tls::PrefixedStream;
use anyhow::{Context, Result, bail};
use courierust::courierust_h1::{parse_request_line, parse_status_line, read_headers};
use courierust::courierust_http::{Method, header::HeaderMap};
use courierust::courierust_io::{BufReader, SliceReader};
use courierust::courierust_ws::frame::{FrameHeader, Mask, OpCode};
use courierust::courierust_ws::handshake;
use ring::rand::{SecureRandom, SystemRandom};
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll, Waker, ready};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

pub const ASTRAVE_WEBSOCKET_PATH: &str = "/~!astrave";

const MAX_HANDSHAKE_BYTES: usize = 8 * 1024;
const MAX_HEADER_LEN: usize = 14;
const MAX_WRITE_PAYLOAD: usize = 64 * 1024;

pub fn normalize_ws_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        ASTRAVE_WEBSOCKET_PATH.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn is_websocket_http_request(peeked: &[u8], path: &str) -> bool {
    let path = normalize_ws_path(path);
    let mut prefix = Vec::with_capacity(5 + path.len());
    prefix.extend_from_slice(b"GET ");
    prefix.extend_from_slice(path.as_bytes());
    peeked.starts_with(&prefix)
}

/// Server side: completes the upgrade and returns the byte tunnel.
pub async fn accept_websocket(stream: tokio::net::TcpStream) -> Result<DynStream> {
    let stream = server_upgrade(stream, None).await?;
    Ok(WsByteStream::server(stream).boxed())
}

pub async fn accept_websocket_on_path(
    stream: tokio::net::TcpStream,
    path: &str,
) -> Result<DynStream> {
    Ok(WsByteStream::server(server_upgrade(stream, Some(path)).await?).boxed())
}

/// Client side: completes the upgrade and returns the byte tunnel.
///
/// `host_header` keeps the configured name (the one the certificate is issued
/// for) even though the socket is already resolved to an address.
pub async fn dial_websocket(
    addr: SocketAddr,
    host_header: &str,
    path: &str,
    keepalive: crate::core::net::TcpKeepaliveConfig,
) -> Result<DynStream> {
    let stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("tcp dial for websocket {addr}"))?;
    crate::core::net::tune_tcp_stream(&stream, keepalive);
    client_websocket_upgrade(stream, host_header, path, None, None).await
}

pub(crate) async fn client_websocket_upgrade<S>(
    mut stream: S,
    host_header: &str,
    path: &str,
    access: Option<&crate::core::config::CloudflareAccess>,
    header_env: Option<&std::collections::BTreeMap<String, String>>,
) -> Result<DynStream>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let path = normalize_ws_path(path);
    if !path.starts_with('/') || !path.bytes().all(|b| b.is_ascii_graphic() && b != b'#') {
        bail!("wsPath must be an HTTP origin-form request target");
    }
    if host_header.is_empty() || !host_header.bytes().all(|b| b.is_ascii_graphic()) {
        bail!("invalid WebSocket Host header");
    }
    let key = handshake::generate_key().map_err(|e| anyhow::anyhow!("websocket key: {e}"))?;
    let mut request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host_header}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n"
    );
    use std::fmt::Write;
    if let Some(access) = access {
        access.validate()?;
        write!(
            request,
            "CF-Access-Client-Id: {}\r\nCF-Access-Client-Secret: {}\r\n",
            access.client_id, access.client_secret
        )?;
    }
    if let Some(headers) = header_env {
        for (name, var) in headers {
            let value = std::env::var(var)
                .with_context(|| format!("missing wsHeaderEnv variable {var}"))?;
            if value.trim().is_empty() || !value.bytes().all(|b| b == b' ' || b.is_ascii_graphic())
            {
                bail!("invalid wsHeaderEnv value for {name}");
            }
            if request.len() + name.len() + value.len() + 6 > MAX_HANDSHAKE_BYTES {
                bail!("WebSocket upgrade headers exceed the handshake limit");
            }
            write!(request, "{name}: {value}\r\n")?;
        }
    }
    request.push_str("\r\n");
    anyhow::ensure!(
        request.len() <= MAX_HANDSHAKE_BYTES,
        "WebSocket upgrade headers exceed the handshake limit"
    );

    stream
        .write_all(request.as_bytes())
        .await
        .context("websocket upgrade write")?;
    stream.flush().await?;

    let (head, extra) = read_http_head(&mut stream).await?;
    let (line, headers) = upgrade_headers(&head)?;
    let (status, _) = parse_status_line(line)?;
    if status.as_u16() != 101 {
        bail!("websocket upgrade rejected: {status}");
    }
    if !handshake::header_has_token(&headers, "connection", "upgrade")
        || !handshake::header_has_token(&headers, "upgrade", "websocket")
    {
        bail!("websocket upgrade response headers missing");
    }
    let expected =
        handshake::accept_key(&key).map_err(|e| anyhow::anyhow!("websocket accept key: {e}"))?;
    if headers
        .get("sec-websocket-accept")
        .and_then(|h| h.to_str().ok())
        != Some(expected.as_str())
    {
        bail!("websocket upgrade returned an invalid Sec-WebSocket-Accept");
    }

    Ok(WsByteStream::client(PrefixedStream::new(extra, stream)).boxed())
}

async fn server_upgrade(
    mut stream: tokio::net::TcpStream,
    path: Option<&str>,
) -> Result<PrefixedStream<tokio::net::TcpStream>> {
    let (head, extra) = read_http_head(&mut stream).await?;
    let (line, headers) = upgrade_headers(&head)?;
    let request = parse_request_line(line)?;
    if request.method != Method::GET {
        bail!("websocket upgrade expects a GET request");
    }
    if let Some(path) = path
        && request.target.as_str() != normalize_ws_path(path)
    {
        bail!("unexpected websocket path");
    }
    if !handshake::header_has_token(&headers, "connection", "upgrade")
        || !handshake::header_has_token(&headers, "upgrade", "websocket")
    {
        bail!("websocket upgrade headers missing");
    }
    let key = headers
        .get("sec-websocket-key")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    if !handshake::is_valid_key(key) {
        bail!("websocket upgrade key is invalid");
    }
    let accept =
        handshake::accept_key(key).map_err(|e| anyhow::anyhow!("websocket accept key: {e}"))?;

    let reply = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\
         \r\n"
    );
    stream
        .write_all(reply.as_bytes())
        .await
        .context("websocket upgrade reply")?;
    stream.flush().await?;
    Ok(PrefixedStream::new(extra, stream))
}

async fn read_http_head<S: AsyncRead + Unpin>(stream: &mut S) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut head = Vec::with_capacity(512);
    let mut chunk = [0u8; 1024];
    while head.len() < MAX_HANDSHAKE_BYTES {
        let scan = head.len().saturating_sub(3);
        let limit = chunk.len().min(MAX_HANDSHAKE_BYTES - head.len());
        let n = stream.read(&mut chunk[..limit]).await?;
        if n == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
        }
        head.extend_from_slice(&chunk[..n]);
        if let Some(pos) = head[scan..].windows(4).position(|s| s == b"\r\n\r\n") {
            let extra = head.split_off(scan + pos + 4);
            return Ok((head, extra));
        }
    }
    bail!("websocket handshake exceeded {MAX_HANDSHAKE_BYTES} bytes")
}

fn upgrade_headers(head: &[u8]) -> Result<(&[u8], HeaderMap)> {
    let line_end = head
        .iter()
        .position(|&b| b == b'\n')
        .context("missing HTTP start line")?
        + 1;
    let (line, fields) = head.split_at(line_end);
    let mut reader = BufReader::new(SliceReader::new(fields), fields.len());
    Ok((line, read_headers(&mut reader)?))
}

pub struct WsByteStream<S> {
    inner: S,
    header: [u8; MAX_HEADER_LEN],
    header_len: usize,
    payload_remaining: u64,
    payload_mask: Option<Mask>,
    payload_offset: u64,
    control: Option<(OpCode, Vec<u8>)>,
    write_frame: Vec<u8>,
    write_pos: usize,
    flush_reply: bool,
    read_waiter: Option<Waker>,
    write_waiter: Option<Waker>,
    client: bool,
    closed: bool,
}

impl<S> WsByteStream<S> {
    pub fn client(inner: S) -> Self {
        Self::new(inner, true)
    }

    pub fn server(inner: S) -> Self {
        Self::new(inner, false)
    }

    fn new(inner: S, client: bool) -> Self {
        Self {
            inner,
            header: [0u8; MAX_HEADER_LEN],
            header_len: 0,
            payload_remaining: 0,
            payload_mask: None,
            payload_offset: 0,
            control: None,
            write_frame: Vec::new(),
            write_pos: 0,
            flush_reply: false,
            read_waiter: None,
            write_waiter: None,
            client,
            closed: false,
        }
    }

    pub fn boxed(self) -> DynStream
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        boxed_stream(self)
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> WsByteStream<S> {
    /// Reads payload bytes, unmasking in place. Control-frame payloads are
    /// diverted into [`WsByteStream::control`] and never reach the caller.
    fn poll_read_payload(
        &mut self,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.payload_remaining == 0 {
            return Poll::Ready(Ok(()));
        }

        if self.control.is_some() {
            let mut chunk = [0u8; 125];
            let want = (self.payload_remaining as usize).min(chunk.len());
            let produced = {
                let mut scratch = ReadBuf::new(&mut chunk[..want]);
                ready!(Pin::new(&mut self.inner).poll_read(cx, &mut scratch))?;
                scratch.filled().len()
            };
            if produced == 0 {
                return Poll::Ready(Err(std::io::ErrorKind::UnexpectedEof.into()));
            }
            if let Some(mask) = self.payload_mask {
                mask.apply(self.payload_offset as usize, &mut chunk[..produced]);
            }
            self.payload_offset += produced as u64;
            self.payload_remaining -= produced as u64;
            if let Some((_, sink)) = self.control.as_mut() {
                sink.extend_from_slice(&chunk[..produced]);
            }
            if self.payload_remaining == 0 {
                let (opcode, payload) = self.control.take().expect("control frame in flight");
                self.handle_control(opcode, payload)?;
            }
            return Poll::Ready(Ok(()));
        }

        let want = (buf.remaining() as u64).min(self.payload_remaining) as usize;
        if want == 0 {
            return Poll::Ready(Ok(()));
        }
        let produced = {
            let mut scratch = ReadBuf::new(buf.initialize_unfilled_to(want));
            ready!(Pin::new(&mut self.inner).poll_read(cx, &mut scratch))?;
            scratch.filled().len()
        };
        if produced == 0 {
            return Poll::Ready(Err(std::io::ErrorKind::UnexpectedEof.into()));
        }
        if let Some(mask) = self.payload_mask {
            mask.apply(
                self.payload_offset as usize,
                buf.initialize_unfilled_to(produced),
            );
        }
        buf.advance(produced);
        self.payload_offset += produced as u64;
        self.payload_remaining -= produced as u64;
        Poll::Ready(Ok(()))
    }

    fn handle_control(&mut self, opcode: OpCode, payload: Vec<u8>) -> std::io::Result<()> {
        match opcode {
            OpCode::Ping => self.queue_frame(OpCode::Pong, &payload)?,
            OpCode::Close => {
                self.closed = true;
                self.queue_frame(OpCode::Close, &payload)?;
            }
            _ => {}
        }
        self.flush_reply = matches!(opcode, OpCode::Ping | OpCode::Close);
        Ok(())
    }

    fn queue_frame(&mut self, opcode: OpCode, payload: &[u8]) -> std::io::Result<()> {
        let mut header = FrameHeader::data(opcode, true, payload.len() as u64);
        header.masked = self.client;
        if self.client {
            SystemRandom::new()
                .fill(&mut header.mask_key)
                .map_err(|_| std::io::Error::other("cannot generate a WebSocket mask"))?;
        }

        let mut raw = [0u8; MAX_HEADER_LEN];
        let header_len = header.write(&mut raw);

        self.write_frame.reserve(header_len + payload.len());
        self.write_frame.extend_from_slice(&raw[..header_len]);
        let start = self.write_frame.len();
        self.write_frame.extend_from_slice(payload);
        if header.masked {
            Mask::new(header.mask_key).apply(0, &mut self.write_frame[start..]);
        }
        Ok(())
    }

    fn poll_write_frame(&mut self, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        while self.write_pos < self.write_frame.len() {
            let written = ready!(
                Pin::new(&mut self.inner).poll_write(cx, &self.write_frame[self.write_pos..])
            )?;
            if written == 0 {
                return Poll::Ready(Err(std::io::ErrorKind::WriteZero.into()));
            }
            self.write_pos += written;
        }
        self.write_frame.clear();
        self.write_pos = 0;
        Poll::Ready(Ok(()))
    }

    // Both halves of a split stream may drive a Pong. A socket keeps only its
    // latest write waker, so notify the other half when output can move again.
    fn poll_output(
        &mut self,
        cx: &mut TaskContext<'_>,
        reading: bool,
    ) -> Poll<std::io::Result<()>> {
        let result = match self.poll_write_frame(cx) {
            Poll::Ready(Ok(())) if self.flush_reply => Pin::new(&mut self.inner).poll_flush(cx),
            result => result,
        };
        let (caller, other) = if reading {
            (&mut self.read_waiter, &mut self.write_waiter)
        } else {
            (&mut self.write_waiter, &mut self.read_waiter)
        };
        if result.is_pending() {
            *caller = Some(cx.waker().clone());
        } else {
            self.flush_reply = false;
            *caller = None;
            if let Some(waker) = other.take() {
                waker.wake();
            }
        }
        result
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for WsByteStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        loop {
            if buf.remaining() == 0 {
                return Poll::Ready(Ok(()));
            }

            if !this.write_frame.is_empty() || this.flush_reply {
                match this.poll_output(cx, true) {
                    Poll::Pending if this.flush_reply => return Poll::Pending,
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    _ => (),
                }
            }

            if this.closed {
                return Poll::Ready(Ok(()));
            }

            if this.payload_remaining > 0 {
                let before = buf.filled().len();
                ready!(this.poll_read_payload(cx, buf))?;
                if buf.filled().len() > before {
                    return Poll::Ready(Ok(()));
                }
                continue;
            }

            if this.header_len < 2 {
                let filled = {
                    let mut scratch = ReadBuf::new(&mut this.header[this.header_len..2]);
                    ready!(Pin::new(&mut this.inner).poll_read(cx, &mut scratch))?;
                    scratch.filled().len()
                };
                if filled == 0 {
                    return if this.header_len == 0 {
                        Poll::Ready(Ok(()))
                    } else {
                        Poll::Ready(Err(std::io::ErrorKind::UnexpectedEof.into()))
                    };
                }
                this.header_len += filled;
                continue;
            }

            let Some(total) = FrameHeader::header_len_hint(&this.header[..this.header_len]) else {
                continue;
            };
            if this.header_len < total {
                let filled = {
                    let mut scratch = ReadBuf::new(&mut this.header[this.header_len..total]);
                    ready!(Pin::new(&mut this.inner).poll_read(cx, &mut scratch))?;
                    scratch.filled().len()
                };
                if filled == 0 {
                    return Poll::Ready(Err(std::io::ErrorKind::UnexpectedEof.into()));
                }
                this.header_len += filled;
                continue;
            }

            let header = match FrameHeader::parse(&this.header[..total]) {
                Ok(Some(header)) => header,
                Ok(None) => continue,
                Err(err) => return Poll::Ready(Err(std::io::Error::other(err.to_string()))),
            };
            this.header_len = 0;

            if let Err(err) = header.check_reserved(false) {
                return Poll::Ready(Err(std::io::Error::other(err.to_string())));
            }

            if header.masked == this.client {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid WebSocket mask direction",
                )));
            }

            this.payload_remaining = header.payload_len;
            this.payload_offset = 0;
            this.payload_mask = header.masked.then(|| Mask::new(header.mask_key));

            if header.opcode.is_control() {
                this.control = Some((
                    header.opcode,
                    Vec::with_capacity(header.payload_len as usize),
                ));
                if header.payload_len == 0 {
                    let (opcode, payload) = this.control.take().expect("just set");
                    this.handle_control(opcode, payload)?;
                }
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for WsByteStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        if this.closed {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "websocket closed",
            )));
        }

        ready!(this.poll_output(cx, false))?;

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let n = buf.len().min(MAX_WRITE_PAYLOAD);
        this.queue_frame(OpCode::Binary, &buf[..n])?;
        // Once copied, these bytes belong to the stream even on a short write.
        if let Poll::Ready(result) = this.poll_write_frame(cx) {
            result?;
        }
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        ready!(this.poll_output(cx, false))?;
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if !this.closed {
            this.closed = true;
            this.queue_frame(OpCode::Close, &1000u16.to_be_bytes())?;
        }
        ready!(this.poll_output(cx, false))?;
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn idle_reader_replies_to_ping_before_peer_sends_application_data() {
        let (socket, mut peer) = tokio::io::duplex(3);
        let mut ws = WsByteStream::client(socket);
        let read = async {
            let mut data = [0; 2];
            ws.read_exact(&mut data).await.unwrap();
            assert_eq!(&data, b"ok");
        };
        let exchange = async {
            peer.write_all(b"\x89\x02hi").await.unwrap();
            let mut pong = [0; 8];
            peer.read_exact(&mut pong).await.unwrap();
            let header = FrameHeader::parse(&pong).unwrap().unwrap();
            assert_eq!(
                (header.opcode, header.masked, header.payload_len),
                (OpCode::Pong, true, 2)
            );
            Mask::new(header.mask_key).apply(0, &mut pong[6..]);
            assert_eq!(&pong[6..], b"hi");
            peer.write_all(b"\x82\x02ok").await.unwrap();
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            tokio::join!(read, exchange);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn bounded_writes_and_close_survive_backpressure() {
        let (socket, mut peer) = tokio::io::duplex(113);
        let mut ws = WsByteStream::server(socket);
        let payload: Vec<_> = (0..=255).cycle().take(MAX_WRITE_PAYLOAD + 17).collect();
        let send = async {
            let n = ws.write(&payload).await.unwrap();
            assert_eq!(n, MAX_WRITE_PAYLOAD);
            ws.write_all(&payload[n..]).await.unwrap();
            ws.shutdown().await.unwrap();
        };
        let receive = async {
            let mut bytes = Vec::new();
            peer.read_to_end(&mut bytes).await.unwrap();
            let mut wire = bytes.as_slice();
            for chunk in payload.chunks(MAX_WRITE_PAYLOAD) {
                let frame = FrameHeader::parse(wire).unwrap().unwrap();
                assert_eq!(
                    (frame.opcode, frame.payload_len),
                    (OpCode::Binary, chunk.len() as u64)
                );
                wire = &wire[frame.header_len..];
                assert_eq!(&wire[..chunk.len()], chunk);
                wire = &wire[chunk.len()..];
            }
            assert_eq!(wire, b"\x88\x02\x03\xe8");
        };
        tokio::join!(send, receive);
    }

    #[tokio::test]
    async fn upgrade_preserves_coalesced_frame_and_split_delimiter() {
        let mut bytes = vec![b'x'; 1022];
        bytes.extend_from_slice(b"\r\n\r\n\x82\x02ok");
        let (mut tx, mut rx) = tokio::io::duplex(2048);
        tx.write_all(&bytes).await.unwrap();
        let (head, extra) = read_http_head(&mut rx).await.unwrap();
        assert_eq!(&head[..], &bytes[..1026]);
        let mut ws = WsByteStream::client(PrefixedStream::new(extra, rx));
        let mut data = [0; 2];
        ws.read_exact(&mut data).await.unwrap();
        assert_eq!(&data, b"ok");
        assert!(
            read_http_head(&mut vec![b'x'; MAX_HANDSHAKE_BYTES].as_slice())
                .await
                .is_err()
        );
    }

    #[test]
    fn detects_the_upgrade_prefix() {
        assert!(is_websocket_http_request(
            b"GET /~!astrave HTTP/1.1",
            "/~!astrave"
        ));
        assert!(!is_websocket_http_request(
            b"GET /other HTTP/1.1",
            "/~!astrave"
        ));
        assert!(is_websocket_http_request(b"GET /~!astrave HTTP/1.1", ""));
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let head = b"GET / HTTP/1.1\r\nUpgrade: WebSocket\r\nConnection: keep-alive\r\nConnection: Upgrade\r\n\r\n";
        let (line, headers) = upgrade_headers(head).unwrap();
        assert_eq!(parse_request_line(line).unwrap().method, Method::GET);
        assert!(handshake::header_has_token(
            &headers,
            "connection",
            "upgrade"
        ));
        assert!(handshake::header_has_token(
            &headers,
            "upgrade",
            "websocket"
        ));
    }
}
