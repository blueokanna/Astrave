#[path = "transport_kcp.rs"]
mod kcp;
#[path = "transport_quic.rs"]
mod quic;
#[path = "transport_stream.rs"]
mod stream;
#[path = "transport_tls.rs"]
mod tls;
#[path = "transport_websocket.rs"]
mod websocket;
#[path = "transport_yamux_mux.rs"]
mod yamux_mux;

pub use kcp::{accept_kcp, bind_kcp_listener, default_kcp_config, dial_kcp};
pub use quic::{QuicBiStream, QuicSession, build_client_endpoint, build_server_endpoint, quic_bi};
pub use stream::{AsyncStream, DynStream, boxed_stream};
pub use tls::{
    ALPN_ASTRAVE, check_and_enable_tls, client_crypto_from_tls_files, client_enable_tls,
    generate_self_signed_cert, install_ring_provider, load_pem_cert_key, new_client_tls_config,
    new_server_tls_config, server_crypto, server_crypto_from_tls_files,
};
pub(crate) use websocket::client_websocket_upgrade;
pub use websocket::{
    ASTRAVE_WEBSOCKET_PATH, WsByteStream, accept_websocket, accept_websocket_on_path,
    dial_websocket, is_websocket_http_request, normalize_ws_path,
};
pub use yamux_mux::{YamuxClient, serve_yamux_session};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Quic,
    Websocket,
    Wss,
    Kcp,
}

impl Protocol {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("tcp") {
            Some(Self::Tcp)
        } else if s.eq_ignore_ascii_case("quic") {
            Some(Self::Quic)
        } else if s.eq_ignore_ascii_case("websocket") || s.eq_ignore_ascii_case("ws") {
            Some(Self::Websocket)
        } else if s.eq_ignore_ascii_case("wss") {
            Some(Self::Wss)
        } else if s.eq_ignore_ascii_case("kcp") {
            Some(Self::Kcp)
        } else {
            None
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Quic => "quic",
            Self::Websocket => "websocket",
            Self::Wss => "wss",
            Self::Kcp => "kcp",
        }
    }
}
