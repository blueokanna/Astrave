use super::stream::{DynStream, boxed_stream};
use anyhow::{Context, Result, bail};
use rcgen::{CertificateParams, KeyPair, SanType};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use rustls_platform_verifier::BuilderVerifierExt;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio_rustls::{TlsAcceptor, TlsConnector};

pub const ALPN_ASTRAVE: &[u8] = b"astrave";
pub const TLS_HANDSHAKE_TYPE: u8 = 0x16;

pub struct GeneratedCert {
    pub certs: Vec<CertificateDer<'static>>,
    pub key: PrivatePkcs8KeyDer<'static>,
}

pub fn generate_self_signed_cert(common_name: &str) -> Result<GeneratedCert> {
    let mut params = CertificateParams::new(vec![common_name.to_string()])?;
    params
        .subject_alt_names
        .push(SanType::DnsName(common_name.try_into()?));
    params
        .subject_alt_names
        .push(SanType::IpAddress(std::net::IpAddr::V4(
            std::net::Ipv4Addr::new(127, 0, 0, 1),
        )));

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivatePkcs8KeyDer::from(key_pair.serialize_der());

    Ok(GeneratedCert {
        certs: vec![cert_der],
        key: key_der,
    })
}

pub fn install_ring_provider() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(())
}

pub fn load_pem_cert_key(
    cert_file: &str,
    key_file: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let certs = CertificateDer::pem_file_iter(cert_file)
        .with_context(|| format!("open certFile {cert_file}"))?
        .collect::<Result<Vec<_>, _>>()
        .context("parse certificate PEM")?;
    if certs.is_empty() {
        bail!("no certificates in {cert_file}");
    }

    let key = PrivateKeyDer::from_pem_file(key_file)
        .with_context(|| format!("load private key from {key_file}"))?;

    Ok((certs, key))
}

fn load_ca_roots(ca_path: &str) -> Result<RootCertStore> {
    let certs = CertificateDer::pem_file_iter(ca_path)
        .with_context(|| format!("open trustedCaFile {ca_path}"))?
        .collect::<Result<Vec<_>, _>>()
        .context("parse CA PEM")?;
    if certs.is_empty() {
        bail!("no CA certificates in {ca_path}");
    }
    let mut roots = RootCertStore::empty();
    for c in certs {
        roots
            .add(c)
            .map_err(|e| anyhow::anyhow!("add CA cert: {e}"))?;
    }
    Ok(roots)
}

pub fn new_server_tls_config(
    cert_file: &str,
    key_file: &str,
    ca_path: &str,
) -> Result<Arc<ServerConfig>> {
    install_ring_provider()?;

    let (certs, key) = if cert_file.trim().is_empty() || key_file.trim().is_empty() {
        tracing::info!(
            "transport.tls: no certFile/keyFile — generating ephemeral self-signed cert"
        );
        let identity = generate_self_signed_cert("astrave-server")?;
        (identity.certs, PrivateKeyDer::Pkcs8(identity.key))
    } else {
        load_pem_cert_key(cert_file, key_file)?
    };

    let builder = if ca_path.trim().is_empty() {
        ServerConfig::builder().with_no_client_auth()
    } else {
        let roots = load_ca_roots(ca_path)?;
        let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .context("build client cert verifier")?;
        ServerConfig::builder().with_client_cert_verifier(verifier)
    };

    let cfg = builder
        .with_single_cert(certs, key)
        .context("build rustls ServerConfig")?;

    Ok(Arc::new(cfg))
}

pub fn new_client_tls_config(
    cert_file: &str,
    key_file: &str,
    ca_path: &str,
) -> Result<Arc<ClientConfig>> {
    install_ring_provider()?;

    let builder = if ca_path.trim().is_empty() {
        ClientConfig::builder().with_platform_verifier()?
    } else {
        let roots = load_ca_roots(ca_path)?;
        let verifier = WebPkiServerVerifier::builder(Arc::new(roots))
            .build()
            .context("build server cert verifier")?;
        ClientConfig::builder().with_webpki_verifier(verifier)
    };

    let cfg = if !cert_file.trim().is_empty() && !key_file.trim().is_empty() {
        let (certs, key) = load_pem_cert_key(cert_file, key_file)?;
        builder
            .with_client_auth_cert(certs, key)
            .context("load client certificate")?
    } else {
        builder.with_no_client_auth()
    };

    Ok(Arc::new(cfg))
}

pub fn server_crypto_from_tls_files(
    cert_file: &str,
    key_file: &str,
    ca_path: &str,
) -> Result<quinn::crypto::rustls::QuicServerConfig> {
    let mut cfg = (*new_server_tls_config(cert_file, key_file, ca_path)?).clone();
    cfg.alpn_protocols = vec![ALPN_ASTRAVE.to_vec()];

    quinn::crypto::rustls::QuicServerConfig::try_from(cfg)
        .map_err(|e| anyhow::anyhow!("QuicServerConfig: {e}"))
}

pub fn client_crypto_from_tls_files(
    cert_file: &str,
    key_file: &str,
    ca_path: &str,
) -> Result<quinn::crypto::rustls::QuicClientConfig> {
    let mut cfg = (*new_client_tls_config(cert_file, key_file, ca_path)?).clone();
    cfg.alpn_protocols = vec![ALPN_ASTRAVE.to_vec()];
    quinn::crypto::rustls::QuicClientConfig::try_from(cfg)
        .map_err(|e| anyhow::anyhow!("QuicClientConfig: {e}"))
}

pub fn server_crypto(
    certs: Vec<CertificateDer<'static>>,
    key: PrivatePkcs8KeyDer<'static>,
) -> Result<quinn::crypto::rustls::QuicServerConfig> {
    install_ring_provider()?;
    let mut cfg = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key.into())
        .context("build rustls ServerConfig")?;
    cfg.alpn_protocols = vec![ALPN_ASTRAVE.to_vec()];
    quinn::crypto::rustls::QuicServerConfig::try_from(cfg)
        .map_err(|e| anyhow::anyhow!("QuicServerConfig: {e}"))
}

pub async fn client_enable_tls(
    stream: DynStream,
    tls_cfg: Arc<ClientConfig>,
    server_name: &str,
) -> Result<DynStream> {
    let name = ServerName::try_from(server_name.to_owned())
        .map_err(|e| anyhow::anyhow!("invalid tls serverName {server_name}: {e}"))?;
    let connector = TlsConnector::from(tls_cfg);
    let tls = connector
        .connect(name, stream)
        .await
        .context("client TLS handshake")?;
    Ok(boxed_stream(tls))
}

pub async fn check_and_enable_tls(
    mut stream: DynStream,
    tls_cfg: Arc<ServerConfig>,
    force: bool,
) -> Result<DynStream> {
    let mut first = [0u8; 1];

    stream
        .read_exact(&mut first)
        .await
        .context("peek TLS first byte")?;

    match first[0] {
        TLS_HANDSHAKE_TYPE => {
            let stream = PrefixedByteStream {
                prefix: Some(first[0]),
                inner: stream,
            };
            let acceptor = TlsAcceptor::from(tls_cfg);
            let tls = acceptor
                .accept(boxed_stream(stream))
                .await
                .context("server TLS handshake")?;
            Ok(boxed_stream(tls))
        }
        _ if force => {
            bail!(
                "transport.tls.force=true but first byte is 0x{:02x} (expected TLS handshake 0x16)",
                first[0]
            );
        }
        _ => Ok(boxed_stream(PrefixedByteStream {
            prefix: Some(first[0]),
            inner: stream,
        })),
    }
}

struct PrefixedByteStream {
    prefix: Option<u8>,
    inner: DynStream,
}

impl AsyncRead for PrefixedByteStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if let Some(b) = self.prefix.take() {
            if buf.remaining() > 0 {
                buf.put_slice(&[b]);
                return std::task::Poll::Ready(Ok(()));
            }
            self.prefix = Some(b);
        }
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedByteStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
