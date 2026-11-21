#[path = "client_plugin_socks5.rs"]
mod socks5;
#[path = "client_plugin_tls_term.rs"]
mod tls_term;

use crate::core::config::PluginConfig;
use crate::core::net::TcpKeepaliveConfig;
use crate::core::transport::DynStream;
use anyhow::{Result, bail};
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PluginContext {
    pub name: String,
    pub cert_common_name: String,
    pub tcp_keepalive: TcpKeepaliveConfig,
}

pub struct ConnectionInfo {
    pub stream: DynStream,
    pub src_addr: String,
    pub src_port: u16,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    async fn handle(&self, conn: ConnectionInfo) -> Result<()>;
}

pub fn create(ctx: PluginContext, cfg: &PluginConfig) -> Result<Arc<dyn Plugin>> {
    match cfg.plugin_type.trim().to_ascii_lowercase().as_str() {
        "tls-term" => {
            let p = tls_term::TlsTermPlugin::new(ctx, cfg)?;
            Ok(Arc::new(p))
        }
        "socks5" => {
            let p = socks5::Socks5Plugin::new(ctx, cfg)?;
            Ok(Arc::new(p))
        }
        other => bail!("unknown client plugin type: {other}"),
    }
}
