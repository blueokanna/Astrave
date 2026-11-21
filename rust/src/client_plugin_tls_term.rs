use super::{ConnectionInfo, Plugin, PluginContext};
use crate::core::config::PluginConfig;
use crate::core::net::TcpKeepaliveConfig;
use crate::core::tls::load_or_generate_https_server_config;
use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

pub struct TlsTermPlugin {
    local_addr: String,
    host_header_rewrite: String,
    acceptor: TlsAcceptor,
    tcp_keepalive: TcpKeepaliveConfig,
}

impl TlsTermPlugin {
    pub fn new(ctx: PluginContext, cfg: &PluginConfig) -> Result<Self> {
        let local_addr = cfg.service.trim().to_string();
        if local_addr.is_empty() {
            bail!("tls-term requires plugin.service (e.g. \"127.0.0.1:8080\")");
        }

        let cn = if ctx.cert_common_name.is_empty() {
            "localhost".to_string()
        } else {
            ctx.cert_common_name.clone()
        };
        let tls_cfg = load_or_generate_https_server_config(&cfg.cert_file, &cfg.key_file, &cn)?;
        let acceptor = TlsAcceptor::from(tls_cfg);

        tracing::info!(
            tunnel = %ctx.name,
            %local_addr,
            rewrite = %cfg.host_header_rewrite,
            "plugin tls-term ready (TLS terminates on agent)"
        );

        Ok(Self {
            local_addr,
            host_header_rewrite: cfg.host_header_rewrite.clone(),
            acceptor,
            tcp_keepalive: ctx.tcp_keepalive,
        })
    }
}

#[async_trait]
impl Plugin for TlsTermPlugin {
    fn name(&self) -> &str {
        "tls-term"
    }

    async fn handle(&self, conn: ConnectionInfo) -> Result<()> {
        let tls = self
            .acceptor
            .accept(conn.stream)
            .await
            .map_err(|e| anyhow!("tls-term TLS accept failed: {e}"))?;

        let mut local = TcpStream::connect(&self.local_addr)
            .await
            .map_err(|e| anyhow!("tls-term dial {}: {e}", self.local_addr))?;
        crate::core::net::tune_tcp_stream(&local, self.tcp_keepalive);

        let (mut tls_r, mut tls_w) = tokio::io::split(tls);
        let (mut headers, body_prefix, ()) =
            crate::core::net::read_request_head(&mut tls_r, |_| Ok(())).await?;
        crate::core::net::forward_request_headers(
            &mut headers,
            &self.host_header_rewrite,
            &conn.src_addr,
            "https",
        )?;

        local.write_all(&headers).await?;
        if !body_prefix.is_empty() {
            local.write_all(&body_prefix).await?;
        }

        tracing::debug!(
            local = %self.local_addr,
            src = %format!("{}:{}", conn.src_addr, conn.src_port),
            "tls-term joining decrypted <-> local HTTP"
        );

        let (mut local_r, mut local_w) = tokio::io::split(local);
        let _ = tokio::try_join!(
            async move { tokio::io::copy(&mut local_r, &mut tls_w).await },
            async move { tokio::io::copy(&mut tls_r, &mut local_w).await },
        );
        Ok(())
    }
}
