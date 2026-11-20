#[path = "net_dns.rs"]
mod dns;
#[path = "net_proxy_protocol.rs"]
mod proxy_protocol;
#[path = "net_tcp.rs"]
mod tcp;
#[path = "net_xff.rs"]
mod xff;

pub use dns::{resolve_endpoint, resolve_host, resolve_socket_addr};
pub use proxy_protocol::{
    PROXY_PROTOCOL_MAX_HEADER, ParsedProxyHeader, PpConsume, addrs_from_start_data_conn,
    build_proxy_protocol_header, parse_proxy_protocol_version, try_consume_proxy_protocol,
};
pub use tcp::{
    DEFAULT_TCP_KEEPALIVE_IDLE_SECS, DEFAULT_TCP_KEEPALIVE_INTERVAL_SECS, TcpKeepaliveConfig,
    enable_keepalive, enable_nodelay, tune_tcp_stream,
};
pub use xff::apply_x_forwarded_for;
