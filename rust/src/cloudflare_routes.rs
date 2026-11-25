use anyhow::{Context, Result, bail, ensure};
use http::{HeaderValue, StatusCode, Uri};
use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{sync::Arc, time::Duration};

pub struct Route {
    pub hostname: String,
    pub path: Option<Regex>,
    pub target: Target,
    pub host_header: Option<HeaderValue>,
    pub server_name: Option<String>,
    pub tls: Option<Arc<rustls::ClientConfig>>,
    pub connect_timeout: Duration,
    pub tls_timeout: Duration,
    pub tcp_keep_alive: Duration,
    pub no_happy_eyeballs: bool,
    pub match_sni_to_host: bool,
}

pub enum Target {
    Http(Uri),
    Tcp(Uri),
    Status(StatusCode),
}

pub struct Routes {
    pub version: i32,
    pub rules: Vec<Arc<Route>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    ingress: Vec<Ingress>,
    #[serde(default, rename = "originRequest", deserialize_with = "null_default")]
    origin_request: Map<String, Value>,
    #[serde(default, rename = "warp-routing")]
    warp: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ingress {
    #[serde(default, deserialize_with = "null_default")]
    hostname: String,
    #[serde(default, deserialize_with = "null_default")]
    path: String,
    service: String,
    #[serde(default, rename = "originRequest", deserialize_with = "null_default")]
    origin_request: Map<String, Value>,
}

fn null_default<'de, D: serde::Deserializer<'de>, T: Deserialize<'de> + Default>(
    deserializer: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

impl Routes {
    pub fn empty() -> Self {
        Self {
            version: -1,
            rules: Vec::new(),
        }
    }

    pub fn parse(version: i32, bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= 2 * 1024 * 1024,
            "Cloudflare configuration exceeds 2 MiB"
        );
        let config: Configuration = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("Invalid Cloudflare ingress configuration"))?;
        ensure!(
            config.warp.is_null() || config.warp.is_object(),
            "Invalid warp-routing configuration"
        );
        ensure!(
            config.warp.get("enabled").is_none_or(Value::is_boolean),
            "Invalid warp-routing.enabled value"
        );
        ensure!(
            !config
                .warp
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "WARP private-network routing is not supported by this connector"
        );
        ensure!(
            !config.ingress.is_empty() && config.ingress.len() <= 1000,
            "Expected 1 through 1000 ingress rules"
        );
        let last = config.ingress.len() - 1;
        let mut rules = Vec::new();
        for (index, rule) in config.ingress.into_iter().enumerate() {
            let hostname = if rule.hostname == "*" {
                String::new()
            } else {
                rule.hostname.to_ascii_lowercase()
            };
            let catch_all = hostname.is_empty() && rule.path.is_empty();
            ensure!(
                !hostname.contains(['/', ':', '@']) && !hostname.chars().any(char::is_whitespace),
                "Ingress hostname must be a host without a scheme, path or port"
            );
            ensure!(
                catch_all == (index == last),
                "The final ingress rule must be the only catch-all rule"
            );
            ensure!(
                !hostname.contains('*')
                    || (hostname.starts_with("*.") && !hostname[2..].contains('*')),
                "Invalid ingress wildcard hostname"
            );
            let path = if rule.path.is_empty() {
                None
            } else {
                ensure!(
                    rule.path.len() <= 4096,
                    "Ingress path expression is too long"
                );
                Some(Regex::new(&rule.path).context("Invalid ingress path expression")?)
            };
            let mut options = config.origin_request.clone();
            options.extend(
                rule.origin_request
                    .into_iter()
                    .filter(|(_, value)| !value.is_null()),
            );
            validate_options(&options)?;
            let target = if let Some(code) = rule.service.strip_prefix("http_status:") {
                let code =
                    StatusCode::from_u16(code.parse().context("Invalid http_status service")?)?;
                ensure!(
                    code.as_u16() >= 200,
                    "http_status requires a final HTTP status"
                );
                Target::Status(code)
            } else {
                let uri: Uri = rule
                    .service
                    .parse()
                    .context("Invalid ingress service URL")?;
                ensure!(
                    uri.host().is_some() && !uri.authority().unwrap().as_str().contains('@'),
                    "Invalid ingress service authority"
                );
                ensure!(
                    uri.path() == "/" || uri.path().is_empty(),
                    "Ingress service must not contain a path"
                );
                ensure!(
                    uri.query().is_none(),
                    "Ingress service must not contain a query"
                );
                match uri.scheme_str() {
                    Some("http" | "https") => Target::Http(uri),
                    Some("tcp" | "ssh" | "rdp" | "smb") => Target::Tcp(uri),
                    _ => bail!(
                        "Unsupported ingress service; use http, https, tcp, ssh, rdp, smb or http_status"
                    ),
                }
            };
            let tls = if matches!(&target, Target::Http(uri) if uri.scheme_str() == Some("https")) {
                let mut cfg = (*crate::core::transport::new_client_tls_config(
                    "",
                    "",
                    text(&options, "caPool").unwrap_or(""),
                )?)
                .clone();
                if flag(&options, "noTLSVerify") {
                    cfg.dangerous()
                        .set_certificate_verifier(Arc::new(OriginNoVerify));
                }
                Some(Arc::new(cfg))
            } else {
                None
            };
            rules.push(Arc::new(Route {
                hostname,
                path,
                target,
                tls,
                host_header: text(&options, "httpHostHeader")
                    .filter(|s| !s.is_empty())
                    .map(HeaderValue::from_str)
                    .transpose()?,
                server_name: text(&options, "originServerName")
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                connect_timeout: duration(&options, "connectTimeout", 30)?,
                tls_timeout: duration(&options, "tlsTimeout", 10)?,
                tcp_keep_alive: duration(&options, "tcpKeepAlive", 30)?,
                no_happy_eyeballs: flag(&options, "noHappyEyeballs"),
                match_sni_to_host: flag(&options, "matchSNItoHost"),
            }));
        }
        Ok(Self { version, rules })
    }

    pub fn find(&self, request: &libcfd::Request) -> Option<Arc<Route>> {
        let path = percent_encoding::percent_decode_str(request.uri.path()).decode_utf8_lossy();
        let host = request
            .headers
            .get(http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<http::uri::Authority>().ok())
            .map(|a| a.host().to_ascii_lowercase())
            .or_else(|| request.uri.host().map(str::to_ascii_lowercase))
            .unwrap_or_default();
        self.rules
            .iter()
            .find(|rule| {
                (rule.hostname.is_empty()
                    || rule.hostname == host
                    || rule.hostname.strip_prefix("*.").is_some_and(|suffix| {
                        host.strip_suffix(suffix)
                            .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
                    }))
                    && rule.path.as_ref().is_none_or(|p| p.is_match(&path))
            })
            .cloned()
    }
}

fn text<'a>(options: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    options.get(key).and_then(Value::as_str)
}
fn flag(options: &Map<String, Value>, key: &str) -> bool {
    options.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn duration(options: &Map<String, Value>, key: &str, default: u64) -> Result<Duration> {
    let seconds = options
        .get(key)
        .filter(|v| !v.is_null())
        .map(|v| v.as_f64().context("Invalid origin timeout"))
        .transpose()?
        .unwrap_or(default as f64);
    ensure!(
        seconds.is_finite() && seconds > 0.0 && seconds <= 3600.0,
        "Origin timeout must be between 0 and 3600 seconds"
    );
    Ok(Duration::from_secs_f64(seconds))
}

fn validate_options(options: &Map<String, Value>) -> Result<()> {
    for (key, value) in options {
        if value.is_null() {
            continue;
        }
        match key.as_str() {
            "httpHostHeader" | "originServerName" | "caPool" => {
                ensure!(value.is_string(), "Invalid origin text option")
            }
            "noTLSVerify" | "noHappyEyeballs" | "matchSNItoHost" => {
                ensure!(value.is_boolean(), "Invalid origin boolean option")
            }
            "connectTimeout" | "tlsTimeout" | "tcpKeepAlive" => {
                duration(options, key, 30)?;
            }
            "keepAliveTimeout" | "keepAliveConnections" => bail!(
                "Origin connection pooling option {key} is not implemented; remove this override"
            ),
            "disableChunkedEncoding" | "http2Origin" | "bastionMode" => ensure!(
                value == false,
                "Unsupported Cloudflare origin option: {key}"
            ),
            "proxyType" => ensure!(value == "", "Unsupported Cloudflare proxyType"),
            "proxyPort" => ensure!(value == 0, "Unsupported Cloudflare proxyPort"),
            "proxyAddress" => ensure!(
                value == "127.0.0.1" || value == "",
                "Unsupported Cloudflare proxyAddress"
            ),
            "access" => ensure!(
                value.is_object()
                    && value.get("required").is_none_or(|v| v == false)
                    && value.get("teamName").is_none_or(|v| v == "")
                    && value
                        .get("audTag")
                        .is_none_or(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty)),
                "Origin Access JWT validation is not implemented"
            ),
            _ => bail!("Unsupported Cloudflare origin option: {key}"),
        }
    }
    Ok(())
}

#[derive(Debug)]
struct OriginNoVerify;
impl rustls::client::danger::ServerCertVerifier for OriginNoVerify {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            signature,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            signature,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_path_wildcard_and_catch_all_routing() {
        let routes = Routes::parse(3, br#"{"ingress":[{"hostname":"*.example.com","path":"^/api/","service":"http://127.0.0.1:8080"},{"hostname":"web.example.com","service":"http_status:403"},{"service":"http_status:404"}]}"#).unwrap();
        for (url, expected) in [
            ("https://web.example.com/api/a", 200),
            ("https://web.example.com/", 403),
            ("https://example.com/api/a", 404),
            ("https://badexample.com/api/a", 404),
        ] {
            let req = libcfd::Request::new(
                http::Method::GET,
                url.parse().unwrap(),
                Default::default(),
                libcfd::Body::empty(),
            );
            let status = match routes.find(&req).unwrap().target {
                Target::Status(s) => s.as_u16(),
                _ => 200,
            };
            assert_eq!(status, expected);
        }
    }
    #[test]
    fn rejects_unsupported_security_and_incomplete_rules() {
        for source in [
            r#"{"ingress":[{"service":"http_status:404","originRequest":{"access":{"required":true}}}]}"#,
            r#"{"ingress":[{"hostname":"example.com","service":"http://localhost"}]}"#,
            r#"{"ingress":[{"service":"http_status:404"}],"warp-routing":{"enabled":true}}"#,
        ] {
            assert!(Routes::parse(1, source.as_bytes()).is_err());
        }
    }

    #[test]
    fn null_options_inherit_and_encoded_paths_match_decoded_routes() {
        let routes = Routes::parse(1, br#"{"originRequest":{"httpHostHeader":"internal.test"},"ingress":[{"path":"^/space here/","service":"http://localhost","originRequest":{"httpHostHeader":null}},{"hostname":"*","path":null,"originRequest":null,"service":"http_status:404"}]}"#).unwrap();
        let request = libcfd::Request::new(
            http::Method::GET,
            "https://example.com/space%20here/a".parse().unwrap(),
            Default::default(),
            libcfd::Body::empty(),
        );
        let route = routes.find(&request).unwrap();
        assert!(matches!(route.target, Target::Http(_)));
        assert_eq!(route.host_header.as_ref().unwrap(), "internal.test");
    }
}
