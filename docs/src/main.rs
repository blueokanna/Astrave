mod releases;

use courierust::{
    courierust_body::Body,
    courierust_http::{method::Method, request::Request, response::Response, status::StatusCode},
    courierust_server::{Handler, Server},
};
use pulldown_cmark::{Options, Parser, html};
use std::{
    env, fs, io,
    path::{Component, Path, PathBuf},
};

fn files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            files(root, &path, out)?;
        } else if entry.file_type()?.is_file() {
            out.push(path.strip_prefix(root).unwrap().to_owned());
        }
    }
    Ok(())
}

fn build(root: &Path, dest: &Path, release: Option<&releases::Release>) -> io::Result<()> {
    for lang in ["en", "zh"] {
        let dir = root.join(lang);
        let mut paths = Vec::new();
        files(&dir, &dir, &mut paths)?;
        paths.retain(|p| p.extension().is_some_and(|x| x == "md"));
        paths.sort();
        let mut contents = String::new();
        for path in paths {
            let source = fs::read_to_string(dir.join(&path))?.replace("\r\n", "\n");
            let body = source
                .strip_prefix("---\n")
                .and_then(|s| s.split_once("\n---\n"))
                .map_or(source.as_str(), |(_, body)| body);
            let title = body
                .lines()
                .find_map(|l| l.strip_prefix("# "))
                .unwrap_or("Astrave");
            let page = path.with_extension("html");
            let relative = page.to_string_lossy().replace('\\', "/");
            let up = "../".repeat(page.components().count() - 1);
            let other = if lang == "en" { "zh" } else { "en" };
            let body = body
                .replace(".md)", ".html)")
                .replace("](/img/", &format!("]({up}../img/"));
            let mut markup = String::new();
            html::push_html(
                &mut markup,
                Parser::new_ext(
                    &body,
                    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
                ),
            );
            if relative == "download.html" {
                markup.push_str(&releases::downloads(release, lang));
            }
            let title = title.replace('&', "&amp;").replace('<', "&lt;");
            let nav = format!(
                "<nav><a href='{up}index.html'>Astrave</a> · <a href='{up}../{other}/{relative}' lang='{other}'>{other}</a></nav>"
            );
            let output = dest.join(lang).join(page);
            fs::create_dir_all(output.parent().unwrap())?;
            fs::write(output, document(lang, &title, &nav, &markup))?;
            contents.push_str(&format!("<li><a href='{relative}'>{title}</a></li>"));
        }
        let (heading, other, switch) = if lang == "en" {
            ("Documentation", "zh", "简体中文")
        } else {
            ("使用文档", "en", "English")
        };
        fs::write(
            dest.join(lang).join("index.html"),
            document(
                lang,
                heading,
                &format!("<a href='../{other}/index.html'>{switch}</a>"),
                &format!("<h1>{heading}</h1><ul>{contents}</ul>"),
            ),
        )?;
    }
    let assets = root.join("static");
    let mut paths = Vec::new();
    files(&assets, &assets, &mut paths)?;
    for path in paths {
        let output = dest.join(&path);
        fs::create_dir_all(output.parent().unwrap())?;
        fs::copy(assets.join(path), output)?;
    }
    fs::write(
        dest.join("index.html"),
        document(
            "en",
            "Astrave",
            "",
            "<h1>Astrave</h1><p><a href='en/index.html'>English documentation</a> · <a href='zh/index.html'>简体中文文档</a></p>",
        ),
    )
}

fn document(lang: &str, title: &str, nav: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang='{lang}'><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>{title} · Astrave</title><style>:root{{color-scheme:light dark}}body{{font:1rem/1.65 system-ui,sans-serif;max-width:60rem;margin:2rem auto;padding:0 1rem}}img{{max-width:100%}}pre,table{{overflow:auto;display:block}}pre,td,th{{padding:.5rem;background:#8881}}code{{overflow-wrap:anywhere}}</style>{nav}<main>{body}</main></html>"
    )
}

// Only files generated under the documentation root can be previewed.
struct Preview(PathBuf);
impl Handler for Preview {
    fn handle(&self, req: Request<Body>) -> Response<Body> {
        let missing = || Response::new(StatusCode::NOT_FOUND).with_body(Body::from("not found"));
        if req.method != Method::GET {
            return missing();
        }
        let relative = req.uri.path().trim_start_matches('/');
        let relative = Path::new(if relative.is_empty() {
            "index.html"
        } else {
            relative
        });
        if !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return missing();
        }
        let Ok(path) = self.0.join(relative).canonicalize() else {
            return missing();
        };
        if !path.starts_with(&self.0) {
            return missing();
        }
        let mime = match path.extension().and_then(|s| s.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("png") => "image/png",
            Some("ico") => "image/x-icon",
            _ => return missing(),
        };
        match fs::read(path) {
            Ok(bytes) => Response::new(StatusCode::OK)
                .header("content-type", mime)
                .with_body(Body::from(bytes)),
            Err(_) => missing(),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dest = root.join("build");
    let release = releases::latest().await?;
    build(root, &dest, release.as_ref())?;
    if env::args().nth(1).as_deref() == Some("--serve") {
        println!("Preview at http://127.0.0.1:8000/");
        Server::bind("127.0.0.1:8000")?
            .serve_background(Preview(dest.canonicalize()?))?
            .join()?;
    }
    Ok(())
}
