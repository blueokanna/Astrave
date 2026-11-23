use anyhow::{Context, Result, ensure};
use astrave_desktop::core::config::{expand_env_placeholders, parse_config_toml};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Action {
    Provider,
    Issue,
    Renew,
}

#[derive(Deserialize)]
struct Config {
    #[serde(rename = "cloudCert")]
    cert: CloudCert,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CloudCert {
    provider: String,
    email: String,
    domains: Vec<String>,
    storage_dir: PathBuf,
    #[serde(default)]
    accept_terms: bool,
    #[serde(default)]
    staging: bool,
    directory: Option<String>,
    #[serde(default)]
    dns_resolvers: Vec<std::net::SocketAddr>,
}

pub fn run(path: &Path, action: Action, lego: &Path) -> Result<()> {
    let raw = std::fs::read_to_string(path).context("reading cloud certificate configuration")?;
    let cert = parse_config_toml::<Config>(&expand_env_placeholders(&raw)?, path)?.cert;
    ensure!(
        !cert.provider.is_empty()
            && cert
                .provider
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "invalid DNS provider code"
    );
    let mut command = Command::new(lego);
    match action {
        Action::Provider => {
            command.args(["dnshelp", "-c", &cert.provider]);
        }
        Action::Issue | Action::Renew => {
            ensure!(
                cert.accept_terms,
                "set cloudCert.acceptTerms after reviewing the CA subscriber agreement"
            );
            ensure!(
                !cert.email.trim().is_empty()
                    && !cert.domains.is_empty()
                    && !cert.storage_dir.as_os_str().is_empty(),
                "email, domains and storageDir are required"
            );
            let storage = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(cert.storage_dir);
            let directory = if let Some(url) = &cert.directory {
                ensure!(
                    !cert.staging && url.starts_with("https://"),
                    "a custom ACME directory requires HTTPS and staging=false"
                );
                url.as_str()
            } else if cert.staging {
                "https://acme-staging-v02.api.letsencrypt.org/directory"
            } else {
                "https://acme-v02.api.letsencrypt.org/directory"
            };
            command
                .args([
                    "--server",
                    directory,
                    "--accept-tos",
                    "--dns",
                    &cert.provider,
                    "--email",
                    &cert.email,
                    "--path",
                ])
                .arg(storage);
            for resolver in cert.dns_resolvers {
                command.arg("--dns.resolvers").arg(resolver.to_string());
            }
            for domain in cert.domains {
                let name = domain.strip_prefix("*.").unwrap_or(&domain);
                ensure!(
                    !name.is_empty()
                        && name.len() <= 253
                        && name.split('.').all(|label| !label.is_empty()
                            && label.len() <= 63
                            && !label.starts_with('-')
                            && !label.ends_with('-')
                            && label
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-')),
                    "invalid certificate DNS name (use ASCII or Punycode)"
                );
                command.arg("--domains").arg(domain);
            }
            command.arg(if matches!(action, Action::Issue) {
                "run"
            } else {
                "renew"
            });
        }
    }
    let status = command
        .status()
        .context("starting lego (install lego or supply --lego PATH)")?;
    ensure!(status.success(), "lego exited with {status}");
    Ok(())
}
