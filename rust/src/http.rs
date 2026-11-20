//! Bounded HTTPS requests for the certificate providers and release metadata.
//! Courierust handles HTTP/1.1 framing. Rustls keeps the OS trust policy used
//! by the tunnel, including Android's JNI-backed certificate verifier.

use anyhow::{Context, Result, bail, ensure};
use courierust::courierust_http::header::HeaderMap;
pub use courierust::courierust_http::{Method, StatusCode, uri::Url};
use courierust::{courierust_h1 as h1, courierust_io as wire};
use hyper_util::client::proxy::matcher::Matcher;
use rustls::{ClientConfig, ClientConnection, StreamOwned, pki_types::ServerName};
use rustls_platform_verifier::BuilderVerifierExt;
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const MAX_BODY: usize = 4 << 20;
static SLOTS: Semaphore = Semaphore::const_new(4);

trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

struct Socket(TcpStream, Instant);

impl Socket {
    fn remaining(&self) -> io::Result<Duration> {
        self.1
            .checked_duration_since(Instant::now())
            .filter(|t| !t.is_zero())
            .ok_or_else(|| io::ErrorKind::TimedOut.into())
    }
}
impl Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.set_read_timeout(Some(self.remaining()?))?;
        self.0.read(buf)
    }
}
impl Write for Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.set_write_timeout(Some(self.remaining()?))?;
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

fn tls(stream: Box<dyn Stream>, host: &str) -> Result<Box<dyn Stream>> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    let config = match CONFIG.get() {
        Some(config) => config,
        None => {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()?
                .with_platform_verifier()?
                .with_no_client_auth();
            CONFIG.get_or_init(|| Arc::new(config))
        }
    };
    let connection = ClientConnection::new(config.clone(), ServerName::try_from(host.to_owned())?)?;
    Ok(Box::new(StreamOwned::new(connection, stream)))
}

struct Reader<'a>(&'a mut dyn Read);
impl wire::Read for Reader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> courierust::Result<usize> {
        Ok(self.0.read(buf)?)
    }
}

fn head(reader: &mut wire::BufReader<Reader<'_>>) -> Result<(StatusCode, HeaderMap)> {
    for _ in 0..16 {
        let line = reader.read_until(b'\n', 8192)?;
        let (status, _) = h1::parse_status_line(&line)?;
        let headers = h1::read_headers(reader)?;
        if !status.is_informational() {
            return Ok((status, headers));
        }
        ensure!(status.as_u16() != 101, "unexpected HTTP upgrade");
    }
    bail!("too many informational HTTP responses")
}

pub struct Response {
    pub status: StatusCode,
    body: Vec<u8>,
}
impl Response {
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        ensure!(self.status.is_success(), "HTTP status {}", self.status);
        Ok(serde_json::from_slice(&self.body)?)
    }
}

fn response(stream: &mut dyn Read) -> Result<Response> {
    let mut reader = wire::BufReader::new(Reader(stream), 8192);
    let (status, headers) = head(&mut reader)?;
    let body = match h1::body_length(&headers, None, Some(status))? {
        h1::BodyLen::Length(len) => h1::read_body_fixed(&mut reader, len, MAX_BODY)?.into_vec(),
        h1::BodyLen::Chunked => h1::read_body_chunked(&mut reader, MAX_BODY)?.into_vec(),
        h1::BodyLen::None if status.as_u16() == 204 || status.as_u16() == 304 => Vec::new(),
        h1::BodyLen::None => {
            let mut body = Vec::new();
            loop {
                let chunk = reader.fill_buf()?;
                if chunk.is_empty() {
                    break;
                }
                ensure!(
                    chunk.len() <= MAX_BODY - body.len(),
                    "HTTP response too large"
                );
                body.extend_from_slice(chunk);
                let count = chunk.len();
                reader.consume(count);
            }
            body
        }
    };
    Ok(Response { status, body })
}

pub async fn request(
    method: Method,
    address: &str,
    headers: &[(&'static str, &str)],
    body: &str,
    timeout: Duration,
) -> Result<Response> {
    let deadline = Instant::now() + timeout;
    let url = Url::parse(address)?;
    ensure!(
        url.scheme == "https" && url.userinfo.is_none(),
        "expected an HTTPS origin"
    );
    let target = url.path_and_query.as_str();
    ensure!(
        target.starts_with('/') && !target.bytes().any(|b| b <= b' ' || b == 127),
        "invalid HTTP request target"
    );
    let mut fields = HeaderMap::new();
    for &(name, value) in headers {
        fields.insert(name.into(), value.into());
    }
    // TC3 signs the exact Host field. Omit the default HTTPS port on the wire.
    let authority = url.authority();
    let host_header = if url.port == 443 {
        authority.strip_suffix(":443").unwrap_or(&authority)
    } else {
        &authority
    };
    fields.insert("host".into(), host_header.into());
    fields.insert(
        "user-agent".into(),
        concat!("astrave/", env!("CARGO_PKG_VERSION")).into(),
    );
    fields.insert("connection".into(), "close".into());
    if !body.is_empty() || method == Method::POST {
        fields.insert("content-length".into(), body.len().to_string().into());
    }
    let mut message = format!("{} {target} HTTP/1.1\r\n", method.as_str()).into_bytes();
    h1::write_headers(&mut message, &fields)?;
    message.extend_from_slice(body.as_bytes());

    let proxy =
        Matcher::from_system().intercept(&address.parse().context("invalid HTTPS address")?);
    let (host, port) = match &proxy {
        Some(p) => {
            let secure = match p.uri().scheme_str() {
                Some("http") => false,
                Some("https") => true,
                _ => bail!("unsupported HTTPS proxy scheme"),
            };
            (
                p.uri()
                    .host()
                    .context("missing proxy host")?
                    .trim_matches(['[', ']'])
                    .to_owned(),
                p.uri().port_u16().unwrap_or(if secure { 443 } else { 80 }),
            )
        }
        None => (url.host.clone(), url.port),
    };
    tokio::time::timeout_at(deadline.into(), async {
        let slot = SLOTS.acquire().await?;
        let socket = tokio::net::TcpStream::connect((host.as_str(), port))
            .await?
            .into_std()?;
        socket.set_nonblocking(false)?;
        tokio::task::spawn_blocking(move || {
            let _slot = slot;
            let mut stream: Box<dyn Stream> = Box::new(Socket(socket, deadline));
            if let Some(proxy) = proxy {
                if proxy.uri().scheme_str() == Some("https") {
                    stream = tls(
                        stream,
                        proxy
                            .uri()
                            .host()
                            .context("missing proxy host")?
                            .trim_matches(['[', ']']),
                    )?;
                }
                let mut connect =
                    format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n").into_bytes();
                let mut fields = HeaderMap::new();
                if let Some(auth) = proxy.basic_auth() {
                    fields.insert("proxy-authorization".into(), auth.to_str()?.into());
                }
                h1::write_headers(&mut connect, &fields)?;
                stream.write_all(&connect)?;
                stream.flush()?;
                let (status, _) = head(&mut wire::BufReader::new(Reader(&mut stream), 8192))?;
                ensure!(status.is_success(), "HTTPS proxy returned {status}");
            }
            let mut stream = tls(stream, &url.host)?;
            stream.write_all(&message)?;
            stream.flush()?;
            response(&mut stream)
        })
        .await?
    })
    .await
    .context("HTTPS request deadline expired")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fragmented_chunked_json_after_informational_response() {
        let mut input = &b"HTTP/1.1 103 Early Hints\r\n\r\nHTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n[1\r\n1\r\n]\r\n0\r\n\r\n"[..];
        assert_eq!(
            response(&mut input).unwrap().json::<Vec<u8>>().unwrap(),
            [1]
        );
    }

    #[test]
    fn rejects_truncation_and_oversized_lengths() {
        for length in [10, MAX_BODY + 1] {
            let bytes = format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\n\r\nx");
            assert!(response(&mut bytes.as_bytes()).is_err());
        }
    }
}
