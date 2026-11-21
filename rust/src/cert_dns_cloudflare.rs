use super::DnsProvider;
use crate::http::Method;
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use nextjson::{NsonDeserialize, Value, json};

pub struct CloudflareDns {
    token: String,
}

impl CloudflareDns {
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
        }
    }

    async fn call<T: for<'de> NsonDeserialize<'de>>(
        &self,
        method: Method,
        url: &str,
        body: &str,
    ) -> Result<CfResp<T>> {
        let auth = format!("Bearer {}", self.token);
        let response: CfResp<T> = super::json(
            method,
            url,
            &[
                ("authorization", &auth),
                ("content-type", "application/json"),
            ],
            body,
        )
        .await?;
        if !response.success {
            bail!("cloudflare DNS error: {:?}", response.errors);
        }
        Ok(response)
    }

    async fn find_zone(&self, domain: &str) -> Result<String> {
        for (name, _) in super::zone_candidates(domain) {
            let url =
                format!("https://api.cloudflare.com/client/v4/zones?name={name}&status=active");
            let resp: CfResp<Vec<CfZone>> = self
                .call(Method::GET, &url, "")
                .await
                .context("cloudflare list zones")?;
            if let Some(z) = resp
                .result
                .context("missing Cloudflare zone list")?
                .into_iter()
                .next()
            {
                return Ok(z.id);
            }
        }
        Err(anyhow!("cloudflare zone not found for {domain}"))
    }
}

#[async_trait]
impl DnsProvider for CloudflareDns {
    async fn upsert_txt(&self, host: &str, value: &str) -> Result<Option<String>> {
        let host = host.trim_end_matches('.');
        let zone_id = self.find_zone(host).await?;

        let list_url = format!(
            "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records?type=TXT&name={host}"
        );
        let listed: CfResp<Vec<CfRecord>> = self
            .call(Method::GET, &list_url, "")
            .await
            .context("cloudflare list txt")?;
        if listed
            .result
            .context("missing Cloudflare TXT list")?
            .iter()
            .any(|r| r.content.trim_matches('"') == value)
        {
            return Ok(None);
        }

        let body = json!({
            "type": "TXT",
            "name": host,
            "content": value,
            "ttl": 120,
        });
        let create_url =
            format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records");
        let created: CfResp<CfRecord> = self
            .call(Method::POST, &create_url, &body.to_string())
            .await
            .context("cloudflare create txt")?;
        let record = created.result.context("missing Cloudflare TXT record")?;
        tracing::info!(%host, "cert: cloudflare txt upserted");
        Ok(Some(format!("{zone_id}:{}", record.id)))
    }

    async fn delete_txt(&self, record_id: &str) -> Result<()> {
        let Some((zone_id, id)) = record_id.split_once(':') else {
            bail!("invalid cloudflare record id");
        };
        let url = format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{id}");
        self.call::<Value>(Method::DELETE, &url, "")
            .await
            .context("cloudflare delete txt")?;
        Ok(())
    }
}

#[derive(Debug, NsonDeserialize)]
struct CfResp<T> {
    success: bool,
    result: Option<T>,
    #[njson(default = "Vec::new")]
    errors: Vec<Value>,
}

#[derive(Debug, NsonDeserialize)]
struct CfZone {
    id: String,
}

#[derive(Debug, NsonDeserialize)]
struct CfRecord {
    id: String,
    #[njson(default)]
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_create_and_error_envelopes() {
        let ok: CfResp<CfRecord> = nextjson::nextdecode(
            br#"{"success":true,"result":{"id":"txt1","content":"challenge"}}"#,
        )
        .unwrap();
        assert_eq!(ok.result.unwrap().id, "txt1");
        let error: CfResp<Vec<CfZone>> = nextjson::nextdecode(br#"{"success":false,"result":null,"errors":[{"code":10000,"message":"Authentication error"}]}"#).unwrap();
        assert!(!error.success);
        assert_eq!(error.errors[0]["code"].as_u64(), Some(10000));
    }
}
