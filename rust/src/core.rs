#[path = "core_auth.rs"]
pub mod auth;
#[path = "config.rs"]
pub mod config;
#[path = "io.rs"]
pub mod io;
#[path = "limit.rs"]
pub mod limit;
#[path = "logging.rs"]
pub mod logging;
#[path = "core_msg.rs"]
pub mod msg;
#[path = "net.rs"]
pub mod net;
#[path = "tls.rs"]
pub mod tls;
#[path = "transport.rs"]
pub mod transport;
#[path = "udp.rs"]
pub mod udp;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
