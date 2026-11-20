use super::model::{Cert, CertFile, Provider, ProviderFile, SecretsFile};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Store {
    root: PathBuf,
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
        }
    }
}

impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(root.join("certs")).context("create certs dir")?;
        fs::create_dir_all(root.join("acme")).context("create acme dir")?;
        Ok(Self { root })
    }

    fn providers_path(&self) -> PathBuf {
        self.root.join("providers.json")
    }

    fn secrets_path(&self) -> PathBuf {
        self.root.join("secrets.json")
    }

    fn certs_index_path(&self) -> PathBuf {
        self.root.join("certs").join("index.json")
    }

    pub fn cert_dir(&self, id: &str) -> PathBuf {
        self.root.join("certs").join(id)
    }

    pub fn cert_pem_path(&self, id: &str) -> PathBuf {
        self.cert_dir(id).join("fullchain.pem")
    }

    pub fn key_pem_path(&self, id: &str) -> PathBuf {
        self.cert_dir(id).join("privkey.pem")
    }

    pub fn acme_account_path(&self) -> PathBuf {
        self.root.join("acme").join("account.json")
    }

    #[allow(dead_code)]
    pub fn acme_staging_account_path(&self) -> PathBuf {
        self.root.join("acme").join("account-staging.json")
    }

    pub fn list_providers(&self) -> Result<Vec<Provider>> {
        Ok(self.load_providers()?.providers)
    }

    pub fn list_certs(&self) -> Result<Vec<Cert>> {
        Ok(self.load_certs()?.certs)
    }

    pub fn get_provider(&self, id: &str) -> Result<Option<Provider>> {
        Ok(self.list_providers()?.into_iter().find(|p| p.id == id))
    }

    pub fn get_secret(&self, provider_id: &str) -> Result<Option<String>> {
        let file = self.load_secrets()?;
        Ok(file.secrets.get(provider_id).cloned())
    }

    pub fn upsert_provider(
        &self,
        mut provider: Provider,
        secret: Option<&str>,
    ) -> Result<Provider> {
        if provider.name.trim().is_empty() {
            bail!("name is required");
        }
        let mut file = self.load_providers()?;
        if provider.id.is_empty() {
            provider.id = new_id("p");
            if secret.map(|s| s.trim().is_empty()).unwrap_or(true) {
                bail!("secret is required");
            }
            file.providers.push(provider.clone());
        } else if let Some(slot) = file.providers.iter_mut().find(|p| p.id == provider.id) {
            *slot = provider.clone();
        } else {
            bail!("provider not found");
        }

        if let Some(s) = secret {
            let s = s.trim();
            if !s.is_empty() {
                let mut secrets = self.load_secrets()?;
                secrets.secrets.insert(provider.id.clone(), s.to_string());
                self.save_secrets(&secrets)?;
            }
        }
        self.save_providers(&file)?;
        tracing::info!(id = %provider.id, vendor = %provider.vendor.as_str(), "cert: provider saved");
        Ok(provider)
    }

    pub fn delete_provider(&self, id: &str) -> Result<()> {
        let mut file = self.load_providers()?;
        let before = file.providers.len();
        file.providers.retain(|p| p.id != id);
        if file.providers.len() == before {
            bail!("provider not found");
        }
        self.save_providers(&file)?;
        let mut secrets = self.load_secrets()?;
        secrets.secrets.remove(id);
        self.save_secrets(&secrets)?;
        tracing::info!(%id, "cert: provider deleted");
        Ok(())
    }

    pub fn add_cert(&self, cert: Cert, fullchain_pem: &str, key_pem: &str) -> Result<()> {
        let dir = self.cert_dir(&cert.id);
        fs::create_dir_all(&dir).with_context(|| format!("mkdir {}", dir.display()))?;
        let cert_path = self.cert_pem_path(&cert.id);
        let key_path = self.key_pem_path(&cert.id);
        fs::write(&cert_path, fullchain_pem)
            .with_context(|| format!("write {}", cert_path.display()))?;
        write_private(&key_path, key_pem.as_bytes())?;

        let mut file = self.load_certs()?;
        file.certs.retain(|c| c.id != cert.id);
        file.certs.insert(0, cert.clone());
        self.save_certs(&file)?;
        tracing::info!(
            id = %cert.id,
            domains = ?cert.domains,
            "cert: certificate saved"
        );
        Ok(())
    }

    pub fn delete_cert(&self, id: &str) -> Result<()> {
        anyhow::ensure!(
            id.starts_with("c_")
                && id[2..].len() == 12
                && id[2..].bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid certificate ID"
        );
        let mut file = self.load_certs()?;
        let before = file.certs.len();
        file.certs.retain(|c| c.id != id);
        if file.certs.len() == before {
            bail!("cert not found");
        }
        self.save_certs(&file)?;
        let dir = self.cert_dir(id);
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
        tracing::info!(%id, "cert: certificate deleted");
        Ok(())
    }

    fn load_providers(&self) -> Result<ProviderFile> {
        read_json_or_default(&self.providers_path())
    }

    fn save_providers(&self, file: &ProviderFile) -> Result<()> {
        write_json_atomic(&self.providers_path(), file)
    }

    fn load_certs(&self) -> Result<CertFile> {
        read_json_or_default(&self.certs_index_path())
    }

    fn save_certs(&self, file: &CertFile) -> Result<()> {
        write_json_atomic(&self.certs_index_path(), file)
    }

    fn load_secrets(&self) -> Result<SecretsFile> {
        read_json_or_default(&self.secrets_path())
    }

    fn save_secrets(&self, file: &SecretsFile) -> Result<()> {
        write_json_atomic(&self.secrets_path(), file)
    }
}

pub fn new_id(prefix: &str) -> String {
    format!(
        "{prefix}_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    )
}

fn read_json_or_default<T>(path: &Path) -> Result<T>
where
    T: for<'de> nextjson::NsonDeserialize<'de> + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(T::default());
    }
    nextjson::nextdecode(raw.as_bytes())
        .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))
}

fn write_json_atomic<T: nextjson::NsonSerialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw =
        nextjson::to_string_pretty(value).map_err(|e| anyhow::anyhow!("serialize JSON: {e}"))?;
    write_private(path, raw.as_bytes())
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut tmp =
        tempfile::NamedTempFile::new_in(path.parent().context("missing parent directory")?)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(())
}
