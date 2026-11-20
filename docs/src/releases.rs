use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use std::{env, error::Error, fmt::Write, time::Duration};

#[derive(Deserialize)]
pub struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
}

pub async fn latest() -> Result<Option<Release>, Box<dyn Error>> {
    let Some(repo) = env::var("GITHUB_REPOSITORY").ok().filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let valid = |s: &str| {
        !matches!(s, "" | "." | "..")
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    };
    if !repo
        .split_once('/')
        .is_some_and(|(owner, name)| valid(owner) && valid(name))
    {
        return Err("GITHUB_REPOSITORY must contain an owner and repository name".into());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = Client::builder()
        .user_agent("astrave-docs")
        .timeout(Duration::from_secs(15))
        .build()?;
    let mut req = client.get(format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    ));
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        req = req.bearer_auth(token);
    }
    let res = req.send().await?;
    if res.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    Ok(Some(res.error_for_status()?.json().await?))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn downloads(release: Option<&Release>, lang: &str) -> String {
    let mut rows = String::new();
    if let Some(release) = release {
        for asset in &release.assets {
            let Ok(url) = Url::parse(&asset.browser_download_url) else {
                continue;
            };
            if url.scheme() == "https"
                && url.host_str() == Some("github.com")
                && url.username().is_empty()
                && url.password().is_none()
            {
                write!(
                    rows,
                    "<tr><td><a href=\"{}\">{}</a></td><td>{:.1}</td></tr>",
                    escape(url.as_str()),
                    escape(&asset.name),
                    asset.size as f64 / 1048576.
                )
                .unwrap();
            }
        }
        if !rows.is_empty() {
            return format!(
                "<table><thead><tr><th>{}</th><th>MiB</th></tr></thead><tbody>{rows}</tbody></table>",
                escape(&release.tag_name)
            );
        }
    }
    if lang == "zh" {
        "<p>当前文档未关联可下载的发布包。</p>"
    } else {
        "<p>This documentation build has no release assets attached.</p>"
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_assets_use_verified_links_and_escaped_labels() {
        let release = Release {
            tag_name: "v1<2".into(),
            assets: vec![
                Asset {
                    name: "client & server.zip".into(),
                    size: 1 << 20,
                    browser_download_url:
                        "https://github.com/owner/project/releases/download/v1/test.zip".into(),
                },
                Asset {
                    name: "invalid asset".into(),
                    size: 0,
                    browser_download_url: "data:text/plain,invalid".into(),
                },
            ],
        };
        let table = downloads(Some(&release), "en");
        assert!(
            table.contains("client &amp; server.zip")
                && table.contains("v1&lt;2")
                && table.contains("<td>1.0</td>")
        );
        assert!(!table.contains("invalid asset"));
        assert!(downloads(None, "zh").contains("发布包"));
    }
}
