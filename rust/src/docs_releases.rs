use astrave_desktop::http::{Method, StatusCode, Url, request};
use nextjson::NsonDeserialize;
use std::{env, error::Error, fmt::Write, time::Duration};

#[derive(NsonDeserialize)]
pub struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(NsonDeserialize)]
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
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let bearer = env::var("GITHUB_TOKEN")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| format!("Bearer {v}"));
    let headers: Vec<_> = bearer
        .as_deref()
        .map(|v| ("authorization", v))
        .into_iter()
        .collect();
    let res = request(Method::GET, &url, &headers, "", Duration::from_secs(15)).await?;
    if res.status == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    Ok(Some(res.json()?))
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
            if url.scheme == "https"
                && url.host == "github.com"
                && url.userinfo.is_none()
                && asset
                    .browser_download_url
                    .starts_with("https://github.com/")
            {
                write!(
                    rows,
                    "<tr><td><a href=\"{}\">{}</a></td><td>{:.1}</td></tr>",
                    escape(&asset.browser_download_url),
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
