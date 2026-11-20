#[path = "config_client.rs"]
mod client;
#[path = "config_env.rs"]
mod env;
#[path = "config_log.rs"]
mod log;
#[path = "config_server.rs"]
mod server;

pub use client::{
    ClientConfig, ClientTlsConfig, PluginConfig, TransportConfig, TunnelConfig,
    TunnelTransportConfig,
};
pub use env::{contains_env_placeholders, expand_env_placeholders};
pub use log::LogConfig;
pub use server::{
    DashboardConfig, QuicConfig, ServerConfig, ServerTlsConfig, ServerTransportConfig,
    parse_host_port,
};

use anyhow::Context;
use std::fs;
use std::path::{Path, PathBuf};

pub const CLIENT_CONFIG_CANDIDATES: &[&str] = &["astrave.toml", "conf/astrave.toml"];

pub fn resolve_client_config_path(explicit: Option<&str>) -> anyhow::Result<PathBuf> {
    if let Some(raw) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(path);
        }
        anyhow::bail!(
            "config file not found: '{raw}'\n\
             \n\
             Specify a valid TOML with -c/--config.\n\
             Example:\n\
               astrave -c conf/astrave.toml"
        );
    }

    for cand in CLIENT_CONFIG_CANDIDATES {
        let path = PathBuf::from(cand);
        if path.is_file() {
            return Ok(path);
        }
    }

    let tried = CLIENT_CONFIG_CANDIDATES.join(", ");
    anyhow::bail!(
        "no config file found (tried: {tried})\n\
         \n\
         A config file is required.\n\
         Specify one with -c/--config.\n\
         Example:\n\
           astrave -c conf/astrave.toml"
    )
}

pub(crate) fn read_toml_file(path: &Path) -> anyhow::Result<String> {
    fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read config file '{}'; specify a valid path with -c/--config",
            path.display()
        )
    })
}

/// Parser diagnostics can quote an entire credentials line, including values
/// expanded from the environment. Only carry its location across this boundary.
pub fn parse_config_toml<T: serde::de::DeserializeOwned>(
    text: &str,
    path: &Path,
) -> anyhow::Result<T> {
    toml::from_str(text).map_err(|err| {
        let Some(span) = err.span() else {
            return anyhow::anyhow!("Invalid TOML in '{}'", path.display());
        };
        let before = &text.as_bytes()[..span.start.min(text.len())];
        let line = before.iter().filter(|&&byte| byte == b'\n').count() + 1;
        anyhow::anyhow!("Invalid TOML in '{}' at line {line}", path.display())
    })
}

pub(crate) fn resolve_maybe_relative(base: &Path, p: &str) -> String {
    let p = p.trim();
    if p.is_empty() {
        return String::new();
    }
    let path = Path::new(p);
    if path.is_absolute() {
        p.to_string()
    } else {
        base.join(path)
            .canonicalize()
            .unwrap_or_else(|_| base.join(path))
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parse_errors_report_location_without_source_values() {
        let malformed = "[auth]\ntoken = 'private-test-value\n";
        let wrong_type = "[dashboard]\nport = 'private-test-value'\n";
        for source in [malformed, wrong_type] {
            let error =
                parse_config_toml::<ServerConfig>(source, Path::new("server.toml")).unwrap_err();
            let message = format!("{error:#?}");
            assert!(message.contains("server.toml") && message.contains("line 2"));
            assert!(!message.contains("private-test-value"));
        }
    }
}
