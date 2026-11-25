use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub token: String,
    pub token_file: String,
    pub tunnel_id: String,
    pub protocol: String,
    pub connections: u8,
    pub region: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            token: String::new(),
            token_file: String::new(),
            tunnel_id: String::new(),
            protocol: "auto".into(),
            connections: 4,
            region: String::new(),
        }
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudflareConfig")
            .field("tunnel_id", &self.tunnel_id)
            .field("protocol", &self.protocol)
            .field("connections", &self.connections)
            .field("token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn resolve_paths(&mut self, base: &Path) {
        self.token_file = crate::core::config::resolve_maybe_relative(base, &self.token_file);
    }

    pub fn credentials(&self) -> Result<libcfd::NamedTunnel> {
        ensure!(
            self.token.is_empty() != self.token_file.is_empty(),
            "cloudflare requires exactly one of token or tokenFile"
        );
        let mut token = self.token.clone();
        if !self.token_file.is_empty() {
            std::fs::File::open(&self.token_file)
                .context("Cannot open Cloudflare tokenFile")?
                .take(16385)
                .read_to_string(&mut token)
                .context("Cannot read Cloudflare tokenFile")?;
        }
        ensure!(token.len() <= 16384, "Cloudflare token exceeds 16 KiB");
        // Library/parser diagnostics may include decoded credential fragments.
        let named = libcfd::NamedTunnel::from_token(token.trim())
            .map_err(|_| anyhow::anyhow!("Invalid Cloudflare Tunnel Token; use the connector token from the tunnel installation page"))?;
        if !self.tunnel_id.is_empty() {
            let expected =
                uuid::Uuid::parse_str(&self.tunnel_id).context("Invalid cloudflare.tunnelId")?;
            ensure!(
                expected.as_bytes() == &named.tunnel_identifier_bytes()?,
                "cloudflare.tunnelId does not match the Tunnel Token"
            );
        }
        Ok(named)
    }

    pub fn transport(&self) -> Result<libcfd::Transport> {
        match self.protocol.as_str() {
            "auto" => Ok(libcfd::Transport::Auto),
            "quic" => Ok(libcfd::Transport::Quic),
            "http2" => Ok(libcfd::Transport::H2),
            _ => anyhow::bail!("cloudflare.protocol must be auto, quic or http2"),
        }
    }

    pub fn validate(&self) -> Result<()> {
        self.credentials()?;
        self.transport()?;
        ensure!(
            (1..=4).contains(&self.connections),
            "cloudflare.connections must be 1 through 4"
        );
        ensure!(
            matches!(self.region.as_str(), "" | "us"),
            "cloudflare.region must be empty or us"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    pub fn config() -> Config {
        Config { token: base64::engine::general_purpose::STANDARD.encode(
            br#"{"a":"test-account","s":"c2VjcmV0","t":"550e8400-e29b-41d4-a716-446655440000"}"#), ..Config::default() }
    }

    #[test]
    fn token_identity_and_errors_do_not_expose_credentials() {
        let mut cfg = config();
        cfg.validate().unwrap();
        assert!(!format!("{cfg:?}").contains(&cfg.token));
        cfg.tunnel_id = "550e8400-e29b-41d4-a716-446655440001".into();
        assert!(
            cfg.validate()
                .unwrap_err()
                .to_string()
                .contains("does not match")
        );
        cfg.token = "private-invalid-token".into();
        assert!(!format!("{:#}", cfg.validate().unwrap_err()).contains(&cfg.token));
    }
}
