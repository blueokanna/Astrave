#[path = "tls_server_cert.rs"]
mod server_cert;
#[path = "tls_sni.rs"]
mod sni;

pub use server_cert::load_or_generate_https_server_config;
pub use sni::{PrefixedStream, peek_client_hello_sni};
