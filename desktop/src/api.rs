use crate::cert::{
    issue,
    model::{Provider, Vendor},
    store::Store,
};
use crate::client_log_layer::ClientUiLogLayer;
use anyhow::{Context, Result, bail};
use astrave_client::{ClientConfig, ClientHandle};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tracing_subscriber::prelude::*;

static CLIENT: LazyLock<ClientHandle> = LazyLock::new(ClientHandle::new);
static CONTROL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static ROOT: OnceLock<PathBuf> = OnceLock::new();
static CERT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static ISSUING: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
static METER: LazyLock<Mutex<crate::process_stats::ProcessMeter>> =
    LazyLock::new(|| Mutex::new(crate::process_stats::ProcessMeter::new()));

pub struct Profile {
    pub path: String,
    pub source: String,
    pub server: String,
    pub transport: String,
    pub tls: bool,
    pub multiplex: bool,
    pub tunnels: Vec<Tunnel>,
}

pub struct Tunnel {
    pub name: String,
    pub protocol: String,
    pub service: String,
    pub public_endpoint: String,
}

pub struct SessionState {
    pub status: String,
    pub error: Option<String>,
    pub lines: Vec<String>,
    pub remote_generation: u64,
    pub remotes: Option<std::collections::HashMap<String, String>>,
}

pub struct ProcessUsage {
    pub cpu_percent: f32,
    pub memory_mib: f64,
}

pub fn process_usage() -> ProcessUsage {
    let sample = METER.lock().unwrap_or_else(|e| e.into_inner()).sample();
    ProcessUsage {
        cpu_percent: sample.cpu_percent,
        memory_mib: sample.memory_bytes as f64 / 1048576.0,
    }
}

pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub account: String,
}

pub struct CertificateInfo {
    pub id: String,
    pub domains: Vec<String>,
    pub expires: String,
    pub chain_path: String,
    pub key_path: String,
}

pub struct Inventory {
    pub providers: Vec<ProviderInfo>,
    pub certificates: Vec<CertificateInfo>,
}

fn store() -> Result<Store> {
    Store::open(ROOT.get().context("Desktop has not initialized")?.clone())
}

pub fn initialize(directory: String) -> Result<String> {
    let root = PathBuf::from(directory);
    fs::create_dir_all(&root)?;
    let path = root.join("astrave.toml");
    if !path.exists() {
        let mut tmp = tempfile::NamedTempFile::new_in(&root)?;
        writeln!(
            tmp,
            "server = \"127.0.0.1:9527\"\nagentId = \"{}\"",
            uuid::Uuid::new_v4()
        )?;
        if let Err(err) = tmp.persist_noclobber(&path)
            && err.error.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(err.error.into());
        }
    }
    if ROOT.get_or_init(|| root.clone()) != &root {
        bail!("Desktop already initialized in a different directory");
    }
    astrave_core::transport::install_ring_provider()?;
    let _ = tracing_subscriber::registry()
        .with(ClientUiLogLayer::new(CLIENT.clone()))
        .try_init();
    Ok(path.display().to_string())
}

const MAX_PROFILE: usize = 2 * 1024 * 1024;

fn read_source(path: &Path) -> Result<String> {
    let mut source = String::new();
    fs::File::open(path)?
        .take((MAX_PROFILE + 1) as u64)
        .read_to_string(&mut source)?;
    if source.len() > MAX_PROFILE {
        bail!("Profile exceeds 2 MiB");
    }
    Ok(source)
}

fn parse_profile(path: &Path, source: String) -> Result<Profile> {
    if source.len() > MAX_PROFILE {
        bail!("Profile exceeds 2 MiB");
    }
    let expanded = astrave_core::config::expand_env_placeholders(&source)?;
    let mut config: ClientConfig = astrave_core::config::parse_config_toml(&expanded, path)?;
    config.prepare_runtime(path);
    config.validate()?;
    let tunnels = config
        .tunnels
        .into_iter()
        .map(|t| Tunnel {
            name: t.name,
            public_endpoint: if t.domains.is_empty() {
                t.remote_port.to_string()
            } else {
                t.domains.join(", ")
            },
            service: t
                .plugin
                .as_ref()
                .filter(|p| !p.service.is_empty())
                .map(|p| p.service.clone())
                .unwrap_or(t.service),
            protocol: t.protocol,
        })
        .collect();
    Ok(Profile {
        path: path.display().to_string(),
        source,
        server: config.server,
        transport: config.transport.protocol,
        tls: config.transport.tls.enable,
        multiplex: config.transport.tcp_mux,
        tunnels,
    })
}

pub fn open_profile(path: String) -> Result<Profile> {
    let path = fs::canonicalize(path)?;
    parse_profile(&path, read_source(&path)?)
}

fn parse_document(source: &str) -> Result<toml_edit::DocumentMut> {
    anyhow::ensure!(source.len() <= MAX_PROFILE, "Profile exceeds 2 MiB");
    source
        .parse()
        .map_err(|_: toml_edit::TomlError| anyhow::anyhow!("Invalid TOML document"))
}

fn take_tunnels(doc: &mut toml_edit::DocumentMut) -> Result<toml_edit::ArrayOfTables> {
    match doc.remove("tunnels") {
        None => Ok(toml_edit::ArrayOfTables::new()),
        Some(item) => item
            .into_array_of_tables()
            .map_err(|_| anyhow::anyhow!("Expected tunnel tables")),
    }
}

pub fn tunnel_source(source: String, index: u32) -> Result<String> {
    let mut doc = parse_document(&source)?;
    let tunnels = take_tunnels(&mut doc)?;
    let tunnel = tunnels.get(index as usize).context("Unknown tunnel")?;
    let mut single = toml_edit::ArrayOfTables::new();
    single.push(tunnel.clone());
    let mut fragment = toml_edit::DocumentMut::new();
    fragment["tunnels"] = toml_edit::Item::ArrayOfTables(single);
    Ok(fragment.to_string())
}

pub fn edit_tunnel(source: String, index: Option<u32>, fragment: Option<String>) -> Result<String> {
    let mut doc = parse_document(&source)?;
    let mut tunnels = take_tunnels(&mut doc)?;
    let replacement = fragment
        .map(|s| -> Result<_> {
            let mut edited = parse_document(&s)?;
            let items = take_tunnels(&mut edited)?;
            anyhow::ensure!(
                edited.is_empty() && items.len() == 1,
                "Enter one [[tunnels]] table"
            );
            Ok(items.get(0).unwrap().clone())
        })
        .transpose()?;
    match (index, replacement) {
        (Some(i), new) => {
            let old = tunnels.get_mut(i as usize).context("Unknown tunnel")?;
            if let Some(new) = new {
                *old = new;
            } else {
                tunnels.remove(i as usize);
            }
        }
        (None, Some(new)) => tunnels.push(new),
        (None, None) => bail!("Choose a tunnel to delete"),
    }
    doc["tunnels"] = toml_edit::Item::ArrayOfTables(tunnels);
    Ok(doc.to_string())
}

pub fn reset_connection_settings(source: String) -> Result<String> {
    let mut doc = parse_document(&source)?;
    for key in [
        "transport",
        "auth",
        "user",
        "udpPacketSize",
        "udp_packet_size",
    ] {
        doc.remove(key);
    }
    doc["server"] = toml_edit::value("127.0.0.1:9527");
    Ok(doc.to_string())
}

pub async fn save_profile(path: String, original: String, source: String) -> Result<Profile> {
    let _lock = CONTROL.lock().await;
    let path = Path::new(&path);
    if read_source(path)? != original {
        bail!("File changed on disk. Reopen it before saving.");
    }
    let profile = parse_profile(path, source)?;
    crate::cert::store::write_private(path, profile.source.as_bytes())?;
    Ok(profile)
}

pub async fn start_client(path: String) -> Result<()> {
    let _lock = CONTROL.lock().await;
    CLIENT.start(ClientConfig::load(&path)?, path.into())
}

pub async fn stop_client() {
    let _lock = CONTROL.lock().await;
    CLIENT.stop().await;
}

pub async fn reload_client(path: String) -> Result<String> {
    let _lock = CONTROL.lock().await;
    let outcome = CLIENT.reload_from_path(Path::new(&path)).await?;
    if !outcome.succeeded() {
        bail!("Reload failed: {:?}", outcome.failed);
    }
    Ok(outcome.level.label().into())
}

pub fn session_state(remote_generation: u64) -> SessionState {
    let changed = CLIENT.tunnel_remotes_if_changed(remote_generation);
    SessionState {
        status: format!("{:?}", CLIENT.status()),
        error: CLIENT.last_error(),
        lines: CLIENT.drain_logs(),
        remote_generation: changed
            .as_ref()
            .map_or(remote_generation, |(generation, _)| *generation),
        remotes: changed.map(|(_, remotes)| remotes),
    }
}

pub async fn inventory() -> Result<Inventory> {
    let _lock = CERT_LOCK.lock().await;
    let db = store()?;
    let providers = db
        .list_providers()?
        .into_iter()
        .map(|p| ProviderInfo {
            id: p.id,
            name: p.name,
            vendor: p.vendor.as_str().into(),
            account: p.access_key_id,
        })
        .collect();
    let certificates = db
        .list_certs()?
        .into_iter()
        .map(|c| CertificateInfo {
            chain_path: db.cert_pem_path(&c.id).display().to_string(),
            key_path: db.key_pem_path(&c.id).display().to_string(),
            id: c.id,
            domains: c.domains,
            expires: c.not_after,
        })
        .collect();
    Ok(Inventory {
        providers,
        certificates,
    })
}

pub async fn save_provider(
    id: String,
    name: String,
    vendor: String,
    account: String,
    secret: String,
) -> Result<()> {
    let _lock = CERT_LOCK.lock().await;
    let vendor = match vendor.as_str() {
        "cloudflare" => Vendor::Cloudflare,
        "aliyun" => Vendor::Aliyun,
        "tencent" => Vendor::Tencent,
        _ => bail!("Unknown DNS provider"),
    };
    if vendor != Vendor::Cloudflare && account.trim().is_empty() {
        bail!("Account ID is required");
    }
    store()?.upsert_provider(
        Provider {
            id,
            name,
            vendor,
            access_key_id: account,
        },
        Some(&secret),
    )?;
    Ok(())
}

pub async fn remove_provider(id: String) -> Result<()> {
    let _lock = CERT_LOCK.lock().await;
    store()?.delete_provider(&id)
}

pub async fn remove_certificate(id: String, profile_path: String) -> Result<()> {
    let _lock = CERT_LOCK.lock().await;
    let db = store()?;
    let config = ClientConfig::load(&profile_path)?;
    let paths = [db.cert_pem_path(&id), db.key_pem_path(&id)].map(fs::canonicalize);
    let in_use = |name: &str| {
        fs::canonicalize(name)
            .ok()
            .is_some_and(|p| paths.iter().flatten().any(|path| *path == p))
    };
    let tls = &config.transport.tls;
    anyhow::ensure!(
        ![&tls.cert_file, &tls.key_file, &tls.trusted_ca_file]
            .into_iter()
            .any(|p| in_use(p))
            && !config
                .tunnels
                .iter()
                .filter_map(|t| t.plugin.as_ref())
                .any(|p| in_use(&p.cert_file) || in_use(&p.key_file)),
        "Certificate is used by the current profile"
    );
    db.delete_cert(&id)
}

pub async fn issue_certificate(provider_id: String, domains: Vec<String>) -> Result<()> {
    let _lock = CERT_LOCK.try_lock().context("Certificate store is busy")?;
    let cancel = Arc::new(AtomicBool::new(false));
    let db = store()?;
    *ISSUING.lock().unwrap_or_else(|e| e.into_inner()) = Some(cancel.clone());
    let issuance = issue::issue(
        &db,
        issue::IssueRequest {
            domains,
            provider_id,
        },
        cancel.clone(),
        |step| CLIENT.push_log(format!("ACME · {step:?}")),
    );
    tokio::pin!(issuance);
    let result = tokio::select! {
        result = &mut issuance => result,
        _ = tokio::time::sleep(issue::ISSUE_TIMEOUT) => {
            cancel.store(true, Ordering::Relaxed);
            // Let the DNS cleanup finish before releasing the store.
            let _ = issuance.await;
            Err(anyhow::anyhow!("Certificate issuance timed out"))
        }
    };
    ISSUING.lock().unwrap_or_else(|e| e.into_inner()).take();
    result.map(|_| ())
}

pub fn cancel_issuance() {
    if let Some(cancel) = ISSUING.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_single_tunnel_preserves_other_tables_and_comments() {
        let source = "# local config\nserver = '127.0.0.1:9527'\n[log]\nlevel = 'info' # keep\n[[tunnels]]\nname = 'one'\nprotocol = 'tcp'\nservice = '127.0.0.1:8000'\nremotePort = 9000\n".to_owned();
        let fragment = tunnel_source(source.clone(), 0).unwrap();
        let edited = edit_tunnel(
            source,
            None,
            Some(fragment.replace("'one'", "'two'").replace("9000", "9001")),
        )
        .unwrap();
        let edited = edit_tunnel(edited, Some(0), Some(fragment.replace("9000", "9010"))).unwrap();
        let edited = edit_tunnel(edited, Some(1), None).unwrap();
        assert!(edited.contains("# local config"));
        assert!(edited.contains("'info' # keep"));
        let p = parse_profile(Path::new("astrave.toml"), edited).unwrap();
        assert_eq!(p.tunnels.len(), 1);
        assert_eq!(p.tunnels[0].public_endpoint, "9010");
    }
}
