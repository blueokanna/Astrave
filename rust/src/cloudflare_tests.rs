use super::{proxy::Proxy, routes::Routes};
use futures_util::io::AsyncReadExt as FuturesRead;
use http::{HeaderMap, Method};
use libcfd::{Body, Request};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_util::{compat::FuturesAsyncReadCompatExt, sync::CancellationToken, task::TaskTracker};

fn proxy(service: &str) -> Proxy {
    let source = serde_json::json!({"ingress":[{"service":service}]});
    Proxy {
        routes: Arc::new(RwLock::new(
            Routes::parse(1, &serde_json::to_vec(&source).unwrap()).unwrap(),
        )),
        cancel: CancellationToken::new(),
        tasks: TaskTracker::new(),
    }
}

#[test]
fn remote_configuration_is_atomic_across_connections_and_rejects_stale_versions() {
    let proxy = proxy("http_status:404");
    let handle = crate::client::ClientHandle::new();
    let apply_a = super::configuration_handler(proxy.routes.clone(), handle.clone());
    let apply_b = super::configuration_handler(proxy.routes.clone(), handle.clone());
    let good = br#"{"ingress":[{"hostname":"web.example.com","service":"http://localhost:8080"},{"service":"http_status:404"}]}"#;
    assert_eq!(apply_a(2, good).unwrap(), 2);
    let bad = br#"{"ingress":[{"service":"http_status:503","originRequest":{"access":{"required":true}}}]}"#;
    assert_eq!(apply_b(3, bad).unwrap_err().0, 2);
    assert_eq!(proxy.routes.read().unwrap().version, 2);
    assert_eq!(handle.tunnel_remotes_if_changed(0).unwrap().1.len(), 1);
    assert!(handle.last_error().unwrap().contains("Access"));
    assert_eq!(apply_b(1, good).unwrap(), 2);
    assert_eq!(
        apply_b(
            3,
            br#"{"ingress":[{"hostname":"*","service":"http_status:410"}]}"#
        )
        .unwrap(),
        3
    );
    assert!(handle.last_error().is_none());
    assert!(handle.tunnel_remotes_if_changed(0).unwrap().1.is_empty());
}

#[cfg(feature = "app")]
#[tokio::test]
async fn https_checks_ca_and_server_name_and_supports_explicit_override() {
    crate::core::transport::install_ring_provider().unwrap();
    let identity = rcgen::generate_simple_self_signed(vec!["origin.test".into()]).unwrap();
    let ca = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(ca.path(), identity.cert.pem()).unwrap();
    let tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![identity.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(identity.signing_key.serialize_der())
                .into(),
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = format!("https://{}", listener.local_addr().unwrap());
    let origin = tokio::spawn(async move {
        let mut successes = 0;
        for _ in 0..4 {
            let (socket, _) = listener.accept().await.unwrap();
            if let Ok(mut stream) = acceptor.accept(socket).await {
                let request = head(&mut stream).await.to_lowercase();
                assert!(request.contains("host: internal.test"));
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await
                    .unwrap();
                successes += 1;
            }
        }
        successes
    });
    for (options, success) in [
        (serde_json::json!({"originServerName":"origin.test"}), false),
        (
            serde_json::json!({"originServerName":"wrong.test","caPool":ca.path()}),
            false,
        ),
        (
            serde_json::json!({"originServerName":"origin.test","caPool":ca.path()}),
            true,
        ),
        (serde_json::json!({"noTLSVerify":true}), true),
    ] {
        let proxy = proxy(&service);
        let _guard = proxy.cancel.clone().drop_guard();
        let mut options = options;
        options["httpHostHeader"] = "internal.test".into();
        let source = serde_json::json!({"ingress":[{"service":service,"originRequest":options}]});
        *proxy.routes.write().unwrap() =
            Routes::parse(1, &serde_json::to_vec(&source).unwrap()).unwrap();
        let request = Request::new(
            Method::GET,
            "https://web.example.com/".parse().unwrap(),
            HeaderMap::new(),
            Body::empty(),
        );
        let result = proxy.http(request).await;
        assert_eq!(result.is_ok(), success);
        if let Ok(mut response) = result {
            assert_eq!(response.body.collect().await.unwrap(), b"ok");
        }
    }
    assert_eq!(origin.await.unwrap(), 2);
}

#[cfg(feature = "app")]
#[test]
fn native_profile_loads_relative_token_file_and_rejects_mixed_modes() {
    use base64::Engine;
    let dir = tempfile::tempdir().unwrap();
    let token = base64::engine::general_purpose::STANDARD.encode(
        br#"{"a":"test-account","s":"c2VjcmV0","t":"550e8400-e29b-41d4-a716-446655440000"}"#,
    );
    std::fs::write(dir.path().join("token.txt"), format!("{token}\n")).unwrap();
    let file = dir.path().join("profile.toml");
    std::fs::write(&file, "[cloudflare]\ntokenFile='token.txt'\n").unwrap();
    let mut cfg = crate::client::ClientConfig::load(&file).unwrap();
    cfg.prepare_runtime(&file);
    cfg.validate().unwrap();
    assert!(!format!("{cfg:?}").contains(&token));
    assert_eq!(
        cfg.cloudflare
            .as_ref()
            .unwrap()
            .credentials()
            .unwrap()
            .tunnel_identifier,
        "550e8400-e29b-41d4-a716-446655440000"
    );
    cfg.cloudflare.as_mut().unwrap().token = token;
    assert!(cfg.validate().is_err());
    std::fs::write(
        &file,
        "[cloudflare]\ntokenFile='token.txt'\n[[tunnels]]\nname='mixed'\nservice='127.0.0.1:80'\n",
    )
    .unwrap();
    assert!(crate::client::ClientConfig::load(&file).is_err());
}

async fn head(stream: &mut (impl tokio::io::AsyncRead + Unpin)) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let byte = stream.read_u8().await.unwrap();
        bytes.push(byte);
        assert!(bytes.len() < 16384);
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn streams_chunked_response_before_origin_finishes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = proxy(&format!("http://{}", listener.local_addr().unwrap()));
    let _guard = proxy.cancel.clone().drop_guard();
    let (release, hold) = tokio::sync::oneshot::channel();
    let origin = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = head(&mut stream).await;
        assert!(request.starts_with("GET /events?q=1 HTTP/1.1"));
        assert!(request.to_lowercase().contains("host: web.example.com"));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n\r\n5\r\nhello\r\n").await.unwrap();
        hold.await.unwrap();
        stream.write_all(b"6\r\n world\r\n0\r\n\r\n").await.unwrap();
    });
    let request = Request::new(
        Method::GET,
        "https://web.example.com/events?q=1".parse().unwrap(),
        HeaderMap::new(),
        Body::empty(),
    );
    let mut response = proxy.http(request).await.unwrap();
    assert_eq!(response.headers.get_all("set-cookie").iter().count(), 2);
    assert!(!response.headers.contains_key("transfer-encoding"));
    let mut first = [0; 5];
    tokio::time::timeout(
        Duration::from_secs(2),
        FuturesRead::read_exact(&mut response.body, &mut first),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(&first, b"hello");
    release.send(()).unwrap();
    assert_eq!(response.body.collect().await.unwrap(), b" world");
    origin.await.unwrap();
    proxy.cancel.cancel();
    proxy.tasks.close();
    proxy.tasks.wait().await;
}

#[tokio::test]
async fn forwards_request_body_host_and_origin_errors() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = proxy(&format!("http://{}", listener.local_addr().unwrap()));
    let _guard = proxy.cancel.clone().drop_guard();
    let origin = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert!(head(&mut stream).await.starts_with("POST /upload HTTP/1.1"));
        let mut body = [0; 7];
        stream.read_exact(&mut body).await.unwrap();
        assert_eq!(&body, b"payload");
        stream
            .write_all(b"HTTP/1.1 418 Teapot\r\nContent-Length: 5\r\n\r\nerror")
            .await
            .unwrap();
    });
    let mut headers = HeaderMap::new();
    headers.insert("content-length", "7".parse().unwrap());
    let mut response = proxy
        .http(Request::new(
            Method::POST,
            "https://web.example.com/upload".parse().unwrap(),
            headers,
            Body::from_bytes(b"payload".to_vec()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status.as_u16(), 418);
    assert_eq!(response.body.collect().await.unwrap(), b"error");
    origin.await.unwrap();
}

fn websocket_request() -> Request {
    let mut headers = HeaderMap::new();
    headers.insert(
        "sec-websocket-key",
        "dGhlIHNhbXBsZSBub25jZQ==".parse().unwrap(),
    );
    headers.insert("sec-websocket-version", "13".parse().unwrap());
    Request::new(
        Method::GET,
        "https://web.example.com/socket".parse().unwrap(),
        headers,
        Body::empty(),
    )
}

#[tokio::test]
async fn websocket_upgrade_preserves_early_frames_and_bidirectional_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = proxy(&format!("http://{}", listener.local_addr().unwrap()));
    let _guard = proxy.cancel.clone().drop_guard();
    let origin = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert!(
            head(&mut stream)
                .await
                .to_lowercase()
                .contains("upgrade: websocket")
        );
        stream.write_all(b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n\x82\x02hi").await.unwrap();
        let mut payload = [0; 4];
        stream.read_exact(&mut payload).await.unwrap();
        assert_eq!(&payload, b"test");
    });
    let connection = proxy.websocket(websocket_request()).await.unwrap().unwrap();
    let mut stream = connection.origin.compat();
    let mut first = [0; 4];
    stream.read_exact(&mut first).await.unwrap();
    assert_eq!(&first, b"\x82\x02hi");
    stream.write_all(b"test").await.unwrap();
    origin.await.unwrap();
}

#[tokio::test]
async fn public_tcp_route_translates_websocket_frames() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::{Message, protocol::Role},
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = proxy(&format!("tcp://{}", listener.local_addr().unwrap()));
    let _guard = proxy.cancel.clone().drop_guard();
    let mut incomplete = websocket_request();
    incomplete.headers.remove("sec-websocket-key");
    let rejected = proxy.websocket(incomplete).await.unwrap().err().unwrap();
    assert_eq!(rejected.status, http::StatusCode::BAD_REQUEST);
    let origin = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 6];
        stream.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"secret");
        stream.write_all(b"reply").await.unwrap();
    });
    let connection = proxy.websocket(websocket_request()).await.unwrap().unwrap();
    let mut ws =
        WebSocketStream::from_raw_socket(connection.origin.compat(), Role::Client, None).await;
    ws.send(Message::Binary(b"secret".to_vec().into()))
        .await
        .unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap().into_data().as_ref(),
        b"reply"
    );
    origin.await.unwrap();
}

#[tokio::test]
async fn websocket_rejection_preserves_origin_status_and_body() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = proxy(&format!("http://{}", listener.local_addr().unwrap()));
    let _guard = proxy.cancel.clone().drop_guard();
    let origin = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        head(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 6\r\n\r\ndenied")
            .await
            .unwrap();
    });
    let mut response = match proxy.websocket(websocket_request()).await.unwrap() {
        Err(response) => response,
        Ok(_) => panic!("rejected upgrade must remain an HTTP response"),
    };
    assert_eq!(response.status.as_u16(), 403);
    assert_eq!(response.body.collect().await.unwrap(), b"denied");
    origin.await.unwrap();
}

#[tokio::test]
async fn invalid_reload_keeps_connector_alive_and_valid_reload_restarts_it() {
    use base64::Engine;
    let token = base64::engine::general_purpose::STANDARD.encode(
        br#"{"a":"test-account","s":"c2VjcmV0","t":"550e8400-e29b-41d4-a716-446655440000"}"#,
    );
    let cfg: crate::client::ClientConfig = crate::core::config::parse_config_toml(
        &format!("[cloudflare]\ntoken='{token}'\n"),
        std::path::Path::new("test.toml"),
    )
    .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
    let (stopped, mut stops) = tokio::sync::mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let shutdown = cancel.clone();
    let initial = cfg.clone();
    let task = tokio::spawn(async move {
        super::run_service_with(
            initial,
            crate::client::ClientHandle::new(),
            shutdown,
            &mut rx,
            move |_, _, cancel| {
                let started = started.clone();
                let stopped = stopped.clone();
                async move {
                    started.send(()).unwrap();
                    cancel.cancelled().await;
                    stopped.send(()).unwrap();
                    Ok(())
                }
            },
        )
        .await
    });
    starts.recv().await.unwrap();
    let mut invalid = cfg.clone();
    invalid.cloudflare.as_mut().unwrap().token = "invalid".into();
    let (reply, result) = tokio::sync::oneshot::channel();
    tx.send(crate::client::service::ReloadRequest {
        cfg: invalid,
        config_path: "test.toml".into(),
        reply,
    })
    .await
    .unwrap();
    assert!(result.await.unwrap().is_err());
    assert!(stops.try_recv().is_err());
    assert!(starts.try_recv().is_err());
    let (reply, result) = tokio::sync::oneshot::channel();
    tx.send(crate::client::service::ReloadRequest {
        cfg,
        config_path: "test.toml".into(),
        reply,
    })
    .await
    .unwrap();
    result.await.unwrap().unwrap();
    stops.recv().await.unwrap();
    starts.recv().await.unwrap();
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    stops.recv().await.unwrap();
}

#[tokio::test]
async fn stop_cancels_outstanding_origin_tasks() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = proxy(&format!("http://{}", listener.local_addr().unwrap()));
    let _guard = proxy.cancel.clone().drop_guard();
    let (accepted, ready) = tokio::sync::oneshot::channel();
    let origin = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        head(&mut stream).await;
        accepted.send(()).unwrap();
        let mut byte = [0];
        stream.read(&mut byte).await.unwrap()
    });
    let running = proxy.clone();
    proxy.spawn(async move {
        let _ = running
            .http(Request::new(
                Method::GET,
                "https://web.example.com/".parse().unwrap(),
                HeaderMap::new(),
                Body::empty(),
            ))
            .await;
    });
    ready.await.unwrap();
    proxy.cancel.cancel();
    proxy.tasks.close();
    tokio::time::timeout(Duration::from_secs(2), proxy.tasks.wait())
        .await
        .unwrap();
    // GET may contain a terminating chunk before EOF; consume all remaining bytes.
    let _ = origin.await.unwrap();
}

#[cfg(feature = "http")]
#[tokio::test]
#[ignore = "Creates a temporary public Cloudflare quick tunnel carrying only a test response"]
async fn live_native_cloudflare_http2_and_quic() {
    crate::core::transport::install_ring_provider().unwrap();
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    let tunnel = libcfd::create_quick_tunnel(&Default::default())
        .await
        .unwrap();
    for transport in [libcfd::Transport::H2, libcfd::Transport::Quic] {
        let tunnel = tunnel.clone();
        let public = tunnel.url();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service = format!("http://{}", listener.local_addr().unwrap());
        let public_client = proxy(&public);
        let proxy = proxy(&service);
        let cancel = proxy.cancel.clone();
        let _guard = cancel.clone().drop_guard();
        proxy.spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                head(&mut stream).await;
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 25\r\nConnection: close\r\n\r\nastrave-native-edge-proof").await;
            }
        });
        let connector = libcfd::EdgeConnector::new(libcfd::EdgeOptions {
            transport,
            grace_period: Duration::from_secs(1),
            configuration_json: serde_json::to_vec(
                &serde_json::json!({"ingress":[{"service":service}],"warp-routing":{}}),
            )
            .unwrap(),
            ..Default::default()
        });
        let shutdown = cancel.clone();
        let origin = libcfd::Origin::http(proxy.clone()).with_websocket(proxy.clone());
        let run = tokio::spawn(async move {
            connector
                .run(
                    libcfd::Tunnel::Quick(tunnel),
                    origin,
                    shutdown.cancelled_owned(),
                )
                .await
        });
        let mut last_error = String::new();
        let verified = tokio::time::timeout(Duration::from_secs(90), async {
            loop {
                let request = Request::new(
                    Method::GET,
                    public.parse().unwrap(),
                    HeaderMap::new(),
                    Body::empty(),
                );
                match tokio::time::timeout(Duration::from_secs(8), public_client.http(request))
                    .await
                {
                    Ok(Ok(mut response)) => {
                        if response.status.as_u16() == 200
                            && response.body.collect().await.unwrap_or_default()
                                == b"astrave-native-edge-proof"
                        {
                            break;
                        }
                        last_error = format!("HTTP {}", response.status);
                    }
                    Ok(Err(error)) => last_error = format!("{error:#}"),
                    Err(_) => last_error = "public request timed out".into(),
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        })
        .await;
        cancel.cancel();
        public_client.cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), run).await;
        assert!(
            verified.is_ok(),
            "Cloudflare {transport:?} public HTTP verification failed: {last_error}"
        );
        result.unwrap().unwrap().unwrap();
        eprintln!("Native Cloudflare {transport:?}: registered and verified public response");
    }
}
