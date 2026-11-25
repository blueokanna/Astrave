#[path = "net_dns.rs"]
mod dns;
#[path = "net_http_head.rs"]
mod http_head;
#[path = "net_proxy_protocol.rs"]
mod proxy_protocol;
#[path = "net_tcp.rs"]
mod tcp;

pub(crate) use dns::{connect_tcp_addresses, connect_tcp_endpoint, ordered_addresses};
pub use dns::{resolve_endpoint, resolve_host, resolve_socket_addr};
pub use http_head::{apply_x_forwarded_for, forward_request_headers, read_request_head};
pub use proxy_protocol::{
    PROXY_PROTOCOL_MAX_HEADER, ParsedProxyHeader, PpConsume, addrs_from_start_data_conn,
    build_proxy_protocol_header, parse_proxy_protocol_version, try_consume_proxy_protocol,
};
pub use tcp::{
    DEFAULT_TCP_KEEPALIVE_IDLE_SECS, DEFAULT_TCP_KEEPALIVE_INTERVAL_SECS, TcpKeepaliveConfig,
    enable_keepalive, enable_nodelay, tune_tcp_stream,
};

pub const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Heartbeat age uses a monotonic clock even when the system clock changes.
pub struct Heartbeat(std::sync::Mutex<std::time::Instant>);

impl Default for Heartbeat {
    fn default() -> Self {
        Self(std::sync::Mutex::new(std::time::Instant::now()))
    }
}

impl Heartbeat {
    pub fn received(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = std::time::Instant::now();
    }
    pub fn age(&self) -> std::time::Duration {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).elapsed()
    }
}
