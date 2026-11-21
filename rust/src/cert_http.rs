use crate::http::{Method, send};
use bytes::Bytes;
use courierust::courierust_http::header::{HeaderMap, HeaderName, HeaderValue};
use http::Request;
use http_body_util::BodyExt;
use instant_acme::{BodyWrapper, BytesResponse, Error, HttpClient};
use std::{future::Future, pin::Pin, time::Duration};

pub(super) struct AcmeHttp;

impl HttpClient for AcmeHttp {
    fn request(
        &self,
        req: Request<BodyWrapper<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = Result<BytesResponse, Error>> + Send>> {
        Box::pin(async move { exchange(req).await.map_err(|e| Error::Other(e.into())) })
    }
}

async fn exchange(req: Request<BodyWrapper<Bytes>>) -> anyhow::Result<BytesResponse> {
    let (parts, body) = req.into_parts();
    let method = Method::from_bytes(parts.method.as_str().as_bytes())?;
    let mut fields = HeaderMap::new();
    for (name, value) in &parts.headers {
        fields.append(
            HeaderName::from_bytes(name.as_str().as_bytes())?,
            HeaderValue::from_bytes(value.as_bytes())?,
        );
    }
    let body = body.collect().await?.to_bytes();
    let response = send(
        method,
        &parts.uri.to_string(),
        fields,
        &body,
        Duration::from_secs(30),
    )
    .await?;
    let mut builder = http::Response::builder().status(response.status.as_u16());
    for (name, value) in response.headers.iter() {
        builder = builder.header(name.as_bytes(), value.as_bytes());
    }
    let (parts, body) = builder.body(Bytes::from(response.body))?.into_parts();
    Ok(BytesResponse {
        parts,
        body: Box::new(body),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires access to the Let's Encrypt staging directory"]
    async fn staging_directory_and_nonce() {
        let req = Request::get(instant_acme::LetsEncrypt::Staging.url())
            .body(BodyWrapper::default())
            .unwrap();
        let mut directory = AcmeHttp.request(req).await.unwrap();
        assert!(directory.parts.status.is_success());
        let body = directory.body.into_bytes().await.unwrap();
        let info: nextjson::Value = nextjson::nextdecode(&body).unwrap();
        let req = Request::head(info["newNonce"].as_str().unwrap())
            .body(BodyWrapper::default())
            .unwrap();
        let mut nonce = AcmeHttp.request(req).await.unwrap();
        assert!(nonce.parts.status.is_success());
        assert!(!nonce.parts.headers["replay-nonce"].is_empty());
        assert!(nonce.body.into_bytes().await.unwrap().is_empty());
    }
}
