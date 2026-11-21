use super::dns;
use super::model::Cert;
use super::store::{self, Store};
use anyhow::{Context, Result, bail};
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder,
    OrderStatus, RetryPolicy,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const ISSUE_TIMEOUT: Duration = Duration::from_secs(360);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum IssueStep {
    Prepare = 0,
    Order = 1,
    DnsWrite = 2,
    DnsWait = 3,
    Validate = 4,
    Finalize = 5,
    Cleanup = 6,
}

pub struct IssueRequest {
    pub domains: Vec<String>,
    pub provider_id: String,
}

struct PendingTxt {
    host: String,
    value: String,
    created_id: Option<String>,
}

// A CNAME delegation changes only the TXT owner. The original identifier is
// still sent to the CA and is always used to check propagation.
fn challenge_domains(entries: &[String]) -> Result<(Vec<String>, BTreeMap<String, String>)> {
    let mut domains = Vec::new();
    let mut owners = BTreeMap::new();
    for entry in entries {
        let (domain, alias) = entry
            .split_once('=')
            .map_or((entry.as_str(), None), |(d, a)| (d, Some(a)));
        let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
        let host = dns::challenge_name(&domain);
        let owner = alias
            .map(|s| s.trim().trim_end_matches('.').to_ascii_lowercase())
            .unwrap_or_else(|| host.clone());
        let valid = |name: &str, underscore: bool| {
            name.len() <= 253
                && name.contains('.')
                && name.split('.').all(|l| {
                    !l.is_empty()
                        && l.len() <= 63
                        && !l.starts_with('-')
                        && !l.ends_with('-')
                        && l.bytes().all(|b| {
                            b.is_ascii_alphanumeric() || b == b'-' || (underscore && b == b'_')
                        })
                })
        };
        if !valid(domain.strip_prefix("*.").unwrap_or(&domain), false) || !valid(&owner, true) {
            bail!("invalid DNS-01 domain or delegated owner: {entry}");
        }
        if let Some(previous) = owners.insert(host, owner.clone())
            && previous != owner
        {
            bail!("conflicting DNS-01 delegations for {domain}");
        }
        if !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    if domains.is_empty() {
        bail!("at least one domain is required");
    }
    Ok((domains, owners))
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    Ok(())
}

async fn until_cancel<T, E>(
    cancel: &AtomicBool,
    fut: impl Future<Output = Result<T, E>>,
) -> Result<T>
where
    E: Into<anyhow::Error>,
{
    tokio::pin!(fut);
    loop {
        tokio::select! {
            biased;
            r = &mut fut => return r.map_err(Into::into),
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                check_cancel(cancel)?;
            }
        }
    }
}

pub async fn issue(
    store: &Store,
    req: IssueRequest,
    cancel: Arc<AtomicBool>,
    mut on_step: impl FnMut(IssueStep) + Send,
) -> Result<Cert> {
    let (domains, owners) = challenge_domains(&req.domains)?;

    let provider = store
        .get_provider(&req.provider_id)?
        .ok_or_else(|| anyhow::anyhow!("provider not found"))?;
    let secret = store
        .get_secret(&req.provider_id)?
        .ok_or_else(|| anyhow::anyhow!("provider secret not found"))?;
    let dns = dns::build_provider(&provider, &secret)?;

    tracing::info!(domains = ?domains, provider = %provider.id, "acme: issue start");

    on_step(IssueStep::Prepare);
    check_cancel(&cancel)?;
    let account = load_or_create_account(store).await?;
    check_cancel(&cancel)?;

    on_step(IssueStep::Order);
    let identifiers: Vec<Identifier> = domains.iter().map(|d| Identifier::Dns(d.clone())).collect();
    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .context("acme new_order")?;
    check_cancel(&cancel)?;

    on_step(IssueStep::DnsWrite);
    let mut pending: Vec<PendingTxt> = Vec::new();
    let result = async {
        {
            let mut authorizations = order.authorizations();
            while let Some(result) = authorizations.next().await {
                check_cancel(&cancel)?;
                let mut authz = result.context("acme authorizations")?;
                match authz.status {
                    AuthorizationStatus::Pending => {}
                    AuthorizationStatus::Valid => continue,
                    other => bail!("unexpected authorization status: {other:?}"),
                }

                let challenge = authz
                    .challenge(ChallengeType::Dns01)
                    .ok_or_else(|| anyhow::anyhow!("dns-01 challenge missing"))?;

                let identifier = challenge.identifier().to_string();
                let value = challenge.key_authorization().dns_value();
                let host = dns::challenge_name(&identifier);
                tracing::info!(%host, "acme: upsert dns txt");
                let owner = owners
                    .get(&host)
                    .context("unexpected ACME authorization name")?;
                let created_id = dns.upsert_txt(owner, &value).await?;
                tracing::info!(%host, %owner, "acme: dns txt ready");
                pending.push(PendingTxt {
                    host,
                    value,
                    created_id,
                });
            }
        }

        on_step(IssueStep::DnsWait);
        for p in &pending {
            check_cancel(&cancel)?;
            dns::wait_txt_propagated(&p.host, &p.value, &cancel)
                .await
                .context("wait dns propagation")?;
        }

        on_step(IssueStep::Validate);
        {
            let mut authorizations = order.authorizations();
            while let Some(result) = authorizations.next().await {
                check_cancel(&cancel)?;
                let mut authz = result.context("acme authorizations")?;
                if authz.status == AuthorizationStatus::Valid {
                    continue;
                }
                let Some(mut challenge) = authz.challenge(ChallengeType::Dns01) else {
                    continue;
                };
                challenge
                    .set_ready()
                    .await
                    .context("acme set_challenge_ready")?;
            }
        }

        let status = until_cancel(
            &cancel,
            order.poll_ready(&RetryPolicy::new().timeout(Duration::from_secs(180))),
        )
        .await
        .context("acme poll_ready")?;
        if status != OrderStatus::Ready {
            bail!("acme order not ready: {status:?}");
        }
        check_cancel(&cancel)?;

        on_step(IssueStep::Finalize);
        let private_key_pem = until_cancel(&cancel, order.finalize())
            .await
            .context("acme finalize")?;
        let cert_chain_pem = until_cancel(
            &cancel,
            order.poll_certificate(&RetryPolicy::new().timeout(Duration::from_secs(120))),
        )
        .await
        .context("acme certificate")?;

        let cert = Cert {
            id: store::new_id("c"),
            domains,
            provider_id: req.provider_id,
            not_after: leaf_expiration(&cert_chain_pem)?,
        };
        store.add_cert(cert.clone(), &cert_chain_pem, &private_key_pem)?;
        tracing::info!(id = %cert.id, "acme: issue ok");
        Ok(cert)
    }
    .await;

    if pending.iter().any(|p| p.created_id.is_some()) {
        on_step(IssueStep::Cleanup);
        for id in pending.iter().filter_map(|p| p.created_id.as_deref()) {
            if let Err(error) = dns.delete_txt(id).await {
                tracing::warn!(%error, record = %id, "acme: cleanup txt failed");
            }
        }
    }
    result
}

fn leaf_expiration(chain: &str) -> Result<String> {
    use courierust::courierust_tls::x509::parse_certificate;
    use rustls::pki_types::{CertificateDer, pem::PemObject};
    let der =
        CertificateDer::from_pem_slice(chain.as_bytes()).context("invalid ACME certificate PEM")?;
    let leaf = parse_certificate(&der)
        .map_err(|e| anyhow::anyhow!("invalid ACME leaf certificate: {e}"))?;
    Ok(tzcraft::Ticks::from_unix_seconds(leaf.not_after, 0)?
        .to_rfc3339(tzcraft::FractionDigits::None))
}

async fn load_or_create_account(store: &Store) -> Result<Account> {
    let path = store.acme_account_path();
    let directory_url = LetsEncrypt::Production.url().to_owned();

    let builder = Account::builder_with_http(Box::new(super::http::AcmeHttp));

    if path.exists() {
        let raw = std::fs::read_to_string(&path).context("read acme account")?;
        let creds: instant_acme::AccountCredentials =
            serde_json::from_str(&raw).context("parse acme account")?;
        let account = builder
            .from_credentials(creds)
            .await
            .context("acme from_credentials")?;
        tracing::info!("acme: production account loaded");
        return Ok(account);
    }

    let (account, creds) = builder
        .create(
            &NewAccount {
                contact: &[],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url,
            None,
        )
        .await
        .context("acme create account")?;

    let raw = serde_json::to_string_pretty(&creds).context("serialize acme account")?;
    store::write_private(&path, raw.as_bytes()).context("write acme account")?;
    tracing::info!("acme: production account created");
    Ok(account)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn dns_cleanup_preserves_existing_txt_on_cancel_and_propagation_timeout() {
        use crate::cert::model::{Provider, Vendor};
        use crate::http::{Method, Response, StatusCode, TEST_HTTP};
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use courierust::courierust_http::header::HeaderMap;
        use serde_json::json;
        use std::sync::Mutex;
        for cancel_at_wait in [true, false] {
            let tmp = tempfile::tempdir().unwrap();
            let store = Store::open(tmp.path().into()).unwrap();
            let provider = store
                .upsert_provider(
                    Provider {
                        id: String::new(),
                        name: "test".into(),
                        vendor: Vendor::Cloudflare,
                        access_key_id: String::new(),
                    },
                    Some("fixture"),
                )
                .unwrap();
            let key = rcgen::KeyPair::generate().unwrap();
            let account = json!({"id":"https://ca.test/account", "key_pkcs8":URL_SAFE_NO_PAD.encode(key.serialize_der()),"urls":{"newNonce":"https://ca.test/nonce","newAccount":"https://ca.test/account","newOrder":"https://ca.test/order"}});
            std::fs::write(store.acme_account_path(), account.to_string()).unwrap();
            let token = "proof";
            let thumb = load_or_create_account(&store)
                .await
                .unwrap()
                .key_thumbprint()
                .to_owned();
            let proof = URL_SAFE_NO_PAD.encode(courierust::courierust_crypto::sha256::sha256(
                format!("{token}.{thumb}").as_bytes(),
            ));
            let records = Arc::new(Mutex::new(BTreeMap::from([
                ("existing".to_string(), proof.clone()),
                ("other".to_string(), "keep".to_string()),
            ])));
            let state = records.clone();
            let exchange = move |method: &Method, url: &str, body: &[u8]| -> Result<Response> {
                let mut headers = HeaderMap::new();
                headers.insert("replay-nonce".into(), "nonce".into());
                let mut records = state.lock().unwrap();
                let payload = if url == "https://ca.test/nonce" {
                    serde_json::Value::Null
                } else if url == "https://ca.test/order" {
                    headers.insert("location".into(), "https://ca.test/order/1".into());
                    json!({"status":"pending", "authorizations":["https://ca.test/auth/first","https://ca.test/auth/second"],"finalize":"https://ca.test/final"})
                } else if let Some(name) = url.strip_prefix("https://ca.test/auth/") {
                    json!({"identifier":{"type":"dns","value":format!("{name}.test")},"status":"pending","challenges":[{"type":"dns-01","url":"https://ca.test/challenge","status":"pending","token":if name == "first" { token } else { "different-proof" }}]})
                } else if url.starts_with("https://api.cloudflare.com/client/v4/zones?") {
                    json!({"success":true,"result":[{"id":"zone"}]})
                } else if url.contains("/dns_records?") {
                    assert!(url.ends_with("name=_validation.zone.test"));
                    let rows: Vec<_> = records
                        .iter()
                        .map(|(id, value)| json!({"id":id,"content":value}))
                        .collect();
                    json!({"success":true,"result":rows})
                } else if method == &Method::POST && url.ends_with("/dns_records") {
                    let record: serde_json::Value = serde_json::from_slice(body)?;
                    assert_eq!(record["name"], "_validation.zone.test");
                    assert!(
                        records
                            .insert("created".into(), record["content"].as_str().unwrap().into())
                            .is_none()
                    );
                    json!({"success":true,"result":{"id":"created"}})
                } else if method == &Method::DELETE && url.ends_with("/dns_records/created") {
                    assert!(records.remove("created").is_some());
                    json!({"success":true,"result":{"id":"created"}})
                } else if url.ends_with("name=_acme-challenge.first.test&type=TXT") {
                    json!({"Status":0,"Answer":[]})
                } else {
                    panic!("unexpected fixture request {method:?} {url}");
                };
                Ok(Response {
                    status: StatusCode::OK,
                    headers,
                    body: payload.to_string().into_bytes(),
                })
            };
            let cancel = Arc::new(AtomicBool::new(false));
            let req = IssueRequest {
                provider_id: provider.id,
                domains: vec![
                    "first.test=_validation.zone.test".into(),
                    "second.test=_validation.zone.test".into(),
                ],
            };
            let mut cleaned = false;
            let result = TEST_HTTP
                .scope(
                    Box::new(exchange),
                    issue(&store, req, cancel.clone(), |step| {
                        if step == IssueStep::DnsWait {
                            assert_eq!(records.lock().unwrap().len(), 3);
                            cancel.store(cancel_at_wait, Ordering::Relaxed);
                        }
                        cleaned |= step == IssueStep::Cleanup;
                    }),
                )
                .await;
            let error = format!("{:#}", result.unwrap_err());
            assert!(
                error.contains(if cancel_at_wait { "cancelled" } else { "150s" }),
                "{error}"
            );
            assert!(cleaned);
            assert_eq!(
                *records.lock().unwrap(),
                BTreeMap::from([("existing".into(), proof), ("other".into(), "keep".into())])
            );
        }
    }

    #[tokio::test]
    async fn loads_legacy_account_with_saved_directory_endpoints() {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let key = rcgen::KeyPair::generate().unwrap();
        let id = "https://acme.example.test/account/7";
        let raw = serde_json::json!({
            "id": id,
            "key_pkcs8": URL_SAFE_NO_PAD.encode(key.serialize_der()),
            "urls": {"newNonce": "https://acme.example.test/nonce", "newAccount": "https://acme.example.test/account", "newOrder": "https://acme.example.test/order"}
        }).to_string();
        std::fs::write(store.acme_account_path(), &raw).unwrap();
        assert_eq!(load_or_create_account(&store).await.unwrap().id(), id);
        assert_eq!(
            std::fs::read_to_string(store.acme_account_path()).unwrap(),
            raw
        );
    }

    #[test]
    fn expiration_comes_from_the_first_certificate_in_a_chain() {
        let key = rcgen::KeyPair::generate().unwrap();
        let certificate = |year| {
            let mut params = rcgen::CertificateParams::default();
            params.not_after = rcgen::date_time_ymd(year, 6, 12);
            params.self_signed(&key).unwrap().pem()
        };
        let chain = certificate(2031) + &certificate(2051);
        assert_eq!(leaf_expiration(&chain).unwrap(), "2031-06-12T00:00:00Z");
        assert_eq!(
            leaf_expiration(&certificate(2051)).unwrap(),
            "2051-06-12T00:00:00Z"
        );
        assert!(
            leaf_expiration("-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----")
                .is_err()
        );
    }

    #[test]
    fn delegated_txt_keeps_certificate_identity_and_rejects_conflicting_owners() {
        let entry = "*.Example.org.=_proof.dns.example".to_owned();
        let (ids, owners) = challenge_domains(std::slice::from_ref(&entry)).unwrap();
        assert_eq!(ids, ["*.example.org"]);
        assert_eq!(owners["_acme-challenge.example.org"], "_proof.dns.example");
        assert!(challenge_domains(&[entry, "example.org".into()]).is_err());
        assert!(challenge_domains(&["a.example=x.example&token=y".into()]).is_err());
    }
}
