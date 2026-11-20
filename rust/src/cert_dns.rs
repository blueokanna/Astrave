#[path = "cert_dns_aliyun.rs"]
mod aliyun;
#[path = "cert_dns_cloudflare.rs"]
mod cloudflare;
#[path = "cert_dns_tencent.rs"]
mod tencent;

use crate::cert::model::{Provider, Vendor};
use crate::http::{Method, request};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use std::time::Duration;

async fn json<T: serde::de::DeserializeOwned>(
    method: Method,
    url: &str,
    headers: &[(&'static str, &str)],
    body: &str,
) -> Result<T> {
    request(method, url, headers, body, Duration::from_secs(20))
        .await?
        .json()
}

#[async_trait]
pub trait DnsProvider: Send + Sync {
    async fn upsert_txt(&self, host: &str, value: &str) -> Result<String>;
    async fn delete_txt(&self, record_id: &str) -> Result<()>;
}

pub fn build_provider(provider: &Provider, secret: &str) -> Result<Box<dyn DnsProvider>> {
    let secret = secret.trim();
    if secret.is_empty() {
        bail!("dns secret is empty");
    }
    match provider.vendor {
        Vendor::Cloudflare => Ok(Box::new(cloudflare::CloudflareDns::new(secret)?)),
        Vendor::Aliyun => {
            if provider.access_key_id.trim().is_empty() {
                bail!("accessKeyId is required for aliyun");
            }
            Ok(Box::new(aliyun::AliyunDns::new(
                &provider.access_key_id,
                secret,
            )?))
        }
        Vendor::Tencent => {
            if provider.access_key_id.trim().is_empty() {
                bail!("secretId is required for tencent");
            }
            Ok(Box::new(tencent::TencentDns::new(
                &provider.access_key_id,
                secret,
            )?))
        }
    }
}

pub fn challenge_name(domain: &str) -> String {
    let domain = domain
        .strip_prefix("*.")
        .unwrap_or(domain)
        .trim_end_matches('.');
    format!("_acme-challenge.{domain}")
}

pub async fn wait_txt_propagated(
    host: &str,
    expected: &str,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    let host = host.trim_end_matches('.');
    let expected = expected.trim().trim_matches('"');
    let endpoints = [
        format!("https://dns.alidns.com/resolve?name={host}&type=TXT"),
        format!("https://doh.pub/dns-query?name={host}&type=TXT"),
        format!("https://cloudflare-dns.com/dns-query?name={host}&type=TXT"),
    ];
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(150);
    let mut attempt = 0u32;
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("cancelled");
        }
        attempt += 1;
        for url in &endpoints {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                bail!("cancelled");
            }
            match tokio::time::timeout_at(deadline, fetch_txt_answers(url))
                .await
                .context("DNS propagation deadline expired")?
            {
                Ok(answers) if answers.iter().any(|a| a.trim_matches('"') == expected) => {
                    tracing::info!(%host, attempt, "acme: dns txt visible");
                    return Ok(());
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!(error = %e, %url, "acme: doh lookup failed");
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "DNS TXT for {host} not visible after 150s (value may not have propagated)"
            );
        }
        if attempt == 1 || attempt.is_multiple_of(5) {
            tracing::info!(%host, attempt, "acme: waiting dns txt propagation");
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

async fn fetch_txt_answers(url: &str) -> Result<Vec<String>> {
    let resp: serde_json::Value = request(
        Method::GET,
        url,
        &[("accept", "application/dns-json")],
        "",
        Duration::from_secs(8),
    )
    .await?
    .json()?;
    let mut out = Vec::new();
    if let Some(arr) = resp.get("Answer").and_then(|v| v.as_array()) {
        for a in arr {
            if a.get("type").and_then(|t| t.as_u64()) != Some(16) {
                continue;
            }
            if let Some(data) = a.get("data").and_then(|d| d.as_str()) {
                out.push(data.to_string());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_and_apex_authorizations_share_a_txt_owner() {
        assert_eq!(
            challenge_name("*.example.test"),
            challenge_name("example.test.")
        );
    }
}
