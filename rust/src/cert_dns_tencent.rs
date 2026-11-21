use super::DnsProvider;
use crate::{http::Method, proto::hex};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use courierust::courierust_crypto::sha256::sha256_hex;
use nextjson::{NsonDeserialize, Value, json};
use ring::hmac;
use tzcraft::{Ticks, Zone};

pub struct TencentDns {
    secret_id: String,
    secret_key: String,
}

impl TencentDns {
    pub fn new(secret_id: &str, secret_key: &str) -> Self {
        Self {
            secret_id: secret_id.to_string(),
            secret_key: secret_key.to_string(),
        }
    }

    async fn call(&self, action: &str, payload: Value) -> Result<Value> {
        let host = "dnspod.tencentcloudapi.com";
        let service = "dnspod";
        let version = "2021-03-23";
        let instant = Ticks::now()?;
        let timestamp = instant.to_unix_seconds()?.0;
        let date = instant.to_zoned(Zone::Utc).date()?.to_iso();
        let body = payload.to_string();
        let hashed_payload = sha256_hex(body.as_bytes());

        let canonical_headers = format!(
            "content-type:application/json; charset=utf-8\nhost:{host}\nx-tc-action:{}\n",
            action.to_lowercase()
        );
        let signed_headers = "content-type;host;x-tc-action";
        let canonical_request =
            format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{hashed_payload}");
        let hashed_canonical = sha256_hex(canonical_request.as_bytes());
        let credential_scope = format!("{date}/{service}/tc3_request");
        let string_to_sign =
            format!("TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{hashed_canonical}");

        let secret_date = hmac_sha256(
            format!("TC3{}", self.secret_key).as_bytes(),
            date.as_bytes(),
        );
        let secret_service = hmac_sha256(secret_date.as_ref(), service.as_bytes());
        let secret_signing = hmac_sha256(secret_service.as_ref(), b"tc3_request");
        let signature =
            hex::encode(hmac_sha256(secret_signing.as_ref(), string_to_sign.as_bytes()).as_ref());

        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders={signed_headers}, Signature={signature}",
            self.secret_id, credential_scope
        );

        let url = format!("https://{host}");
        let headers = [
            ("authorization", authorization.as_str()),
            ("content-type", "application/json; charset=utf-8"),
            ("x-tc-action", action),
            ("x-tc-timestamp", &timestamp.to_string()),
            ("x-tc-version", version),
        ];
        let v: TcResp = super::json(Method::POST, &url, &headers, &body)
            .await
            .context("tencent DNS request")?;
        v.into_result()
    }

    async fn find_domain(&self, host: &str) -> Result<(u64, String, String)> {
        let listed = self
            .call("DescribeDomainList", json!({"Limit": 100, "Offset": 0}))
            .await?;
        let domains = listed
            .get("DomainList")
            .and_then(|v| v.as_array())
            .map(Vec::as_slice)
            .unwrap_or_default();
        for (name, rr) in super::zone_candidates(host) {
            for d in domains {
                let dn = d.get("Name").and_then(|v| v.as_str()).unwrap_or("");
                let id = d
                    .get("DomainId")
                    .and_then(|v| v.as_u64())
                    .or_else(|| {
                        d.get("DomainId")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok())
                    })
                    .unwrap_or(0);
                if !dn.is_empty() && dn.eq_ignore_ascii_case(name) {
                    tracing::info!(%host, domain = %dn, domain_id = id, %rr, "cert: tencent resolve zone");
                    return Ok((id, dn.to_string(), rr.to_owned()));
                }
            }
        }
        Err(anyhow!("tencent domain not found for {host}"))
    }
}

#[async_trait]
impl DnsProvider for TencentDns {
    async fn upsert_txt(&self, host: &str, value: &str) -> Result<Option<String>> {
        let (domain_id, domain, rr) = self.find_domain(host).await?;

        let mut list_body = json!({
            "Domain": domain,
            "Subdomain": rr,
            "RecordType": "TXT",
            "Limit": 100,
        });
        if domain_id > 0 {
            list_body
                .as_object_mut()
                .expect("object literal")
                .insert("DomainId".into(), domain_id.into());
        }
        let listed = self.call("DescribeRecordList", list_body).await;
        if let Ok(listed) = listed
            && let Some(records) = listed.get("RecordList").and_then(|v| v.as_array())
        {
            for r in records {
                let content = r.get("Value").and_then(|v| v.as_str()).unwrap_or("");
                let id = r.get("RecordId").and_then(|v| v.as_u64()).unwrap_or(0);
                let record_rr = r.get("Name").and_then(|v| v.as_str()).unwrap_or("");
                if !record_rr.is_empty() && record_rr != rr {
                    continue;
                }
                if content == value && id > 0 {
                    tracing::info!(%host, %id, "cert: tencent txt already present");
                    return Ok(None);
                }
            }
        }

        let mut create_body = json!({
            "Domain": domain,
            "SubDomain": rr,
            "RecordType": "TXT",
            "RecordLine": "默认",
            "Value": value,
            "TTL": 600,
        });
        if domain_id > 0 {
            create_body
                .as_object_mut()
                .expect("object literal")
                .insert("DomainId".into(), domain_id.into());
        }
        let created = self.call("CreateRecord", create_body).await?;
        let id = created
            .get("RecordId")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("tencent missing RecordId"))?;
        tracing::info!(%host, %id, %domain, "cert: tencent txt upserted");
        Ok(Some(format!("{domain}:{id}")))
    }

    async fn delete_txt(&self, record_id: &str) -> Result<()> {
        let (domain, id) = match record_id.split_once(':') {
            Some((d, i)) => (
                d.to_string(),
                i.parse::<u64>().context("tencent record id")?,
            ),
            None => {
                bail!("tencent record id missing domain prefix");
            }
        };
        self.call(
            "DeleteRecord",
            json!({
                "Domain": domain,
                "RecordId": id,
            }),
        )
        .await
        .context("tencent delete txt")?;
        Ok(())
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> hmac::Tag {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), data)
}

#[derive(NsonDeserialize)]
struct TcResp {
    #[njson(rename = "Response")]
    response: Value,
}

impl TcResp {
    fn into_result(self) -> Result<Value> {
        let fields = self
            .response
            .as_object()
            .context("invalid Tencent DNS response")?;
        if let Some(error) = fields.get("Error").filter(|value| !value.is_null()) {
            let code = error["Code"]
                .as_str()
                .context("missing Tencent error code")?;
            let message = error["Message"]
                .as_str()
                .context("missing Tencent error message")?;
            bail!("tencent dns error: {code} - {message}");
        }
        Ok(self.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_dnspod_results_and_errors_in_http_successes() {
        let ok: TcResp =
            nextjson::nextdecode(br#"{"Response":{"RecordId":123,"RequestId":"req"}}"#).unwrap();
        assert_eq!(ok.into_result().unwrap()["RecordId"].as_u64(), Some(123));
        let err: TcResp = nextjson::nextdecode(br#"{"Response":{"Error":{"Code":"AuthFailure","Message":"invalid credential"},"RequestId":"req"}}"#).unwrap();
        assert!(
            err.into_result()
                .unwrap_err()
                .to_string()
                .contains("AuthFailure")
        );
    }
}
