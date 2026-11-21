use anyhow::{Context, Result, bail, ensure};
use std::io::Write;
use tokio::io::{AsyncRead, AsyncReadExt};

const MAX_HEAD: usize = 64 * 1024;

pub fn apply_x_forwarded_for(head: &mut Vec<u8>, client_ip: &str, proto: &str) -> Result<()> {
    forward_request_headers(head, "", client_ip, proto).map(|_| ())
}

/// Apply routing headers, preserving all unrelated bytes. Returns whether the request supplied Host.
pub fn forward_request_headers(
    head: &mut Vec<u8>,
    host: &str,
    client_ip: &str,
    proto: &str,
) -> Result<bool> {
    for value in [host, client_ip, proto] {
        ensure!(
            !value.contains(['\r', '\n']),
            "invalid forwarded header value"
        );
    }
    let mut lines = head.split_inclusive(|&b| b == b'\n');
    let mut out = Vec::with_capacity(head.len());
    let mut request = lines.next().context("empty HTTP request")?;
    while matches!(request, b"\r\n" | b"\n") {
        out.extend_from_slice(request);
        request = lines.next().context("missing HTTP request line")?;
    }
    out.extend_from_slice(request);
    let mut fields = Vec::new();
    let ending = loop {
        let line = lines.next().context("unterminated HTTP request head")?;
        if matches!(line, b"\r\n" | b"\n") {
            break line;
        }
        fields.push(line);
    };
    let named = |line: &[u8], name: &[u8]| {
        line.get(..name.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(name))
    };
    let host_index = fields.iter().position(|line| named(line, b"host:"));
    let xff_index = fields
        .iter()
        .rposition(|line| named(line, b"x-forwarded-for:"));
    let has_proto = fields.iter().any(|line| named(line, b"x-forwarded-proto:"));
    for (index, line) in fields.into_iter().enumerate() {
        if !host.is_empty() && Some(index) == host_index {
            write!(out, "Host: {host}")?;
        } else if !client_ip.is_empty() && Some(index) == xff_index {
            let prior = line[b"x-forwarded-for:".len()..].trim_ascii();
            out.extend_from_slice(b"X-Forwarded-For: ");
            if !prior.is_empty() {
                out.extend_from_slice(prior);
                out.extend_from_slice(b", ");
            }
            out.extend_from_slice(client_ip.as_bytes());
        } else {
            out.extend_from_slice(line);
            continue;
        }
        out.extend_from_slice(ending);
    }
    for (insert, name, value) in [
        (host_index.is_none(), "Host", host),
        (xff_index.is_none(), "X-Forwarded-For", client_ip),
        (!has_proto, "X-Forwarded-Proto", proto),
    ] {
        if insert && !value.is_empty() {
            write!(out, "{name}: {value}")?;
            out.extend_from_slice(ending);
        }
    }
    out.extend_from_slice(ending);
    for line in lines {
        out.extend_from_slice(line);
    }
    *head = out;
    Ok(host_index.is_some())
}

/// Inspect one complete request head and retain any body bytes read alongside it for forwarding.
pub async fn read_request_head<R: AsyncRead + Unpin, T>(
    stream: &mut R,
    inspect: impl FnOnce(&httparse::Request<'_, '_>) -> Result<T>,
) -> Result<(Vec<u8>, Vec<u8>, T)> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0; 2048];
    let mut start = 0;
    loop {
        let limit = chunk.len().min(MAX_HEAD - buf.len());
        if limit == 0 {
            bail!("http headers too large");
        }
        let count = stream.read(&mut chunk[..limit]).await?;
        if count == 0 {
            bail!("client closed before http headers completed");
        }
        let scan = buf.len().saturating_sub(2);
        buf.extend_from_slice(&chunk[..count]);
        // httparse accepts leading blank lines and both LF and CRLF delimiters.
        while start < buf.len() && matches!(buf[start], b'\r' | b'\n') {
            start += 1;
        }
        let tail = &buf[scan.max(start)..];
        if !tail.windows(2).any(|s| s == b"\n\n") && !tail.windows(3).any(|s| s == b"\n\r\n") {
            continue;
        }
        let mut fields = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut fields);
        if let httparse::Status::Complete(end) = req.parse(&buf)? {
            let metadata = inspect(&req)?;
            let body = buf.split_off(end);
            return Ok((buf, body, metadata));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarding_preserves_raw_fields_and_existing_proxy_chain() {
        let mut bytes = b"\nPOST / HTTP/1.1\nHoSt: old\nX-Raw: \xff\nX-Forwarded-For: a\nx-forwarded-for: b, c\nX-Forwarded-Proto: https\n\n\x80".to_vec();
        assert!(forward_request_headers(&mut bytes, "new", "d", "http").unwrap());
        assert_eq!(bytes, b"\nPOST / HTTP/1.1\nHost: new\nX-Raw: \xff\nX-Forwarded-For: a\nX-Forwarded-For: b, c, d\nX-Forwarded-Proto: https\n\n\x80");
        let mut missing = b"GET / HTTP/1.0\r\n\r\n".to_vec();
        assert!(
            !forward_request_headers(&mut missing, "example.org", "127.0.0.1", "https").unwrap()
        );
        assert_eq!(missing, b"GET / HTTP/1.0\r\nHost: example.org\r\nX-Forwarded-For: 127.0.0.1\r\nX-Forwarded-Proto: https\r\n\r\n");
    }

    #[tokio::test]
    async fn preserves_fragmented_proxy_request_and_binary_body() {
        for eol in ["\n", "\r\n"] {
            let head =
                format!("{eol}CONNECT example.org:443 HTTP/1.1{eol}Host: example.org{eol}{eol}");
            let body = b"\x00\xff\r\n";
            let message = [head.as_bytes(), body].concat();
            for split in 0..=head.len() {
                let (first, second) = message.split_at(split);
                let mut stream = first.chain(second);
                let (raw, mut data, target) =
                    read_request_head(&mut stream, |r| Ok(r.path.unwrap().to_owned()))
                        .await
                        .unwrap();
                assert_eq!(target, "example.org:443");
                assert_eq!(raw, head.as_bytes());
                stream.read_to_end(&mut data).await.unwrap();
                assert_eq!(data, body);
            }
        }
    }

    #[tokio::test]
    async fn limits_only_header_bytes_and_fields() {
        let line = "POST / HTTP/1.1\r\n";
        for length in [MAX_HEAD - 1, MAX_HEAD, MAX_HEAD + 1] {
            let mut head =
                format!("{line}X: {}\r\n\r\n", "a".repeat(length - line.len() - 7)).into_bytes();
            head.extend_from_slice(b"body");
            let mut stream = head.as_slice().take(17).chain(&head[17..]);
            let result = read_request_head(&mut stream, |_| Ok(())).await;
            if length > MAX_HEAD {
                assert!(result.is_err());
            } else {
                let (raw, mut body, ()) = result.unwrap();
                assert_eq!(raw.len(), length);
                stream.read_to_end(&mut body).await.unwrap();
                assert_eq!(body, b"body");
            }
        }
        for count in [64, 65] {
            let head = format!("{line}{}\r\n", "X: y\r\n".repeat(count));
            assert_eq!(
                read_request_head(&mut head.as_bytes(), |_| Ok(()))
                    .await
                    .is_ok(),
                count == 64
            );
        }
    }
}
