use super::DnsProvider;
use crate::{http::Method, proto::base64};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use nextjson::Value;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use ring::hmac;
use std::collections::BTreeMap;
use tzcraft::{FractionDigits, Ticks};
use uuid::Uuid;

const ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub struct AliyunDns {
    access_key_id: String,
    access_key_secret: String,
}

impl AliyunDns {
    pub fn new(access_key_id: &str, access_key_secret: &str) -> Self {
        Self {
            access_key_id: access_key_id.to_string(),
            access_key_secret: access_key_secret.to_string(),
        }
    }

    fn sign(&self, canonical: &str) -> String {
        let string_to_sign = format!("GET&%2F&{}", enc(canonical));
        let key = hmac::Key::new(
            hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
            format!("{}&", self.access_key_secret).as_bytes(),
        );
        base64::encode(hmac::sign(&key, string_to_sign.as_bytes()).as_ref())
    }

    async fn call(&self, mut params: BTreeMap<String, String>) -> Result<Value> {
        params.insert("Format".into(), "JSON".into());
        params.insert("Version".into(), "2015-01-09".into());
        params.insert("AccessKeyId".into(), self.access_key_id.clone());
        params.insert("SignatureMethod".into(), "HMAC-SHA1".into());
        params.insert(
            "Timestamp".into(),
            Ticks::now()?.to_rfc3339(FractionDigits::None),
        );
        params.insert("SignatureVersion".into(), "1.0".into());
        params.insert("SignatureNonce".into(), Uuid::new_v4().to_string());
        let query: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
            .collect::<Vec<_>>()
            .join("&");
        let signature = enc(&self.sign(&query));
        let url = format!("https://alidns.aliyuncs.com/?{query}&Signature={signature}");
        let v: Value = super::json(Method::GET, &url, &[], "")
            .await
            .context("aliyun DNS request")?;
        if let Some(code) = v.get("Code").and_then(|v| v.as_str()) {
            bail!("aliyun DNS error: {code}");
        }
        Ok(v)
    }

    async fn find_domain(&self, host: &str) -> Result<(String, String)> {
        for (name, rr) in super::zone_candidates(host) {
            let mut params = BTreeMap::new();
            params.insert("Action".into(), "DescribeDomainInfo".into());
            params.insert("DomainName".into(), name.to_owned());
            if self.call(params).await.is_ok() {
                return Ok((name.to_owned(), rr.to_owned()));
            }
        }
        Err(anyhow!("aliyun domain not found for {host}"))
    }
}

#[async_trait]
impl DnsProvider for AliyunDns {
    async fn upsert_txt(&self, host: &str, value: &str) -> Result<Option<String>> {
        let (domain, rr) = self.find_domain(host).await?;
        tracing::info!(%host, %domain, %rr, "cert: aliyun resolve zone");

        let mut params = BTreeMap::new();
        params.insert("Action".into(), "DescribeDomainRecords".into());
        params.insert("DomainName".into(), domain.clone());
        params.insert("RRKeyWord".into(), rr.clone());
        params.insert("Type".into(), "TXT".into());
        let listed = self.call(params).await?;
        for r in domain_records(&listed) {
            let content = r.get("Value").and_then(|v| v.as_str()).unwrap_or("");
            let id = r.get("RecordId").and_then(|v| v.as_str()).unwrap_or("");
            let record_rr = r.get("RR").and_then(|v| v.as_str()).unwrap_or("");
            if record_rr != rr || id.is_empty() {
                continue;
            }
            if content == value {
                tracing::info!(%host, %id, "cert: aliyun txt already present");
                return Ok(None);
            }
        }

        let mut params = BTreeMap::new();
        params.insert("Action".into(), "AddDomainRecord".into());
        params.insert("DomainName".into(), domain);
        params.insert("RR".into(), rr);
        params.insert("Type".into(), "TXT".into());
        params.insert("Value".into(), value.to_string());
        params.insert("TTL".into(), "600".into());
        let created = self.call(params).await?;
        let id = created
            .get("RecordId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("aliyun missing RecordId"))?
            .to_string();
        tracing::info!(%host, %id, "cert: aliyun txt upserted");
        Ok(Some(id))
    }

    async fn delete_txt(&self, record_id: &str) -> Result<()> {
        let mut params = BTreeMap::new();
        params.insert("Action".into(), "DeleteDomainRecord".into());
        params.insert("RecordId".into(), record_id.to_string());
        self.call(params).await.context("aliyun delete txt")?;
        Ok(())
    }
}

fn domain_records(listed: &Value) -> &[Value] {
    match listed.pointer("/DomainRecords/Record") {
        Some(Value::Array(arr)) => arr,
        Some(obj @ Value::Object(_)) => std::slice::from_ref(obj),
        _ => &[],
    }
}

fn enc(s: &str) -> String {
    utf8_percent_encode(s, ENCODE_SET).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_single_and_multiple_record_shapes() {
        for records in [
            r#"{"RecordId":"r1","Value":"challenge"}"#,
            r#"[{"RecordId":"r1","Value":"challenge"}]"#,
        ] {
            let json = format!(r#"{{"DomainRecords":{{"Record":{records}}}}}"#);
            let reply = nextjson::nextdecode(json.as_bytes()).unwrap();
            assert_eq!(domain_records(&reply)[0]["RecordId"].as_str(), Some("r1"));
        }
    }

    #[test]
    fn signature_preserves_aliyun_query_escaping() {
        let dns = AliyunDns::new("test-id", "test-secret");
        let query = format!("Action=DescribeDomainRecords&RRKeyWord={}", enc("a b+*~"));
        assert_eq!(dns.sign(&query), "vn0/n1c6Ei64daMX1Ci0/o2L+8Q=");
    }
}
