use crate::control::Control;
use std::collections::{BTreeMap, HashMap};
use std::ops::Bound::{Included, Unbounded};
use std::sync::{Arc, RwLock, Weak};

pub struct HttpRoute {
    pub tunnel_name: String,
    pub control: Weak<Control>,
    pub host_header_rewrite: String,
    pub basic_auth_user: String,
    pub basic_auth_password: String,
    pub limiter: Option<std::sync::Arc<astrave_desktop::core::limit::BandwidthLimiter>>,
}

type Locations = BTreeMap<Box<[u8]>, Arc<HttpRoute>>;

pub struct HttpGw {
    routes: RwLock<HashMap<String, Locations>>,
    pub listen_port: u16,
}

impl HttpGw {
    pub fn new(listen_port: u16) -> Self {
        Self {
            routes: RwLock::new(HashMap::new()),
            listen_port,
        }
    }

    pub fn register(
        &self,
        domains: &[String],
        locations: &[String],
        route: HttpRoute,
    ) -> anyhow::Result<()> {
        let mut map = self.routes.write().unwrap_or_else(|e| e.into_inner());
        for domain in domains {
            if let Some(paths) = map.get(domain) {
                for path in locations {
                    if paths
                        .get(path.as_bytes())
                        .is_some_and(|r| r.tunnel_name != route.tunnel_name)
                    {
                        anyhow::bail!("router config conflict: domain={domain} location={path}");
                    }
                }
            }
        }
        let route = Arc::new(route);
        for domain in domains {
            let paths = map.entry(domain.clone()).or_default();
            for path in locations {
                paths.insert(path.as_bytes().into(), Arc::clone(&route));
            }
        }
        Ok(())
    }

    pub fn unregister_tunnel(&self, domains: &[String], tunnel_name: &str) {
        let mut map = self.routes.write().unwrap_or_else(|e| e.into_inner());
        for domain in domains {
            if let Some(paths) = map.get_mut(domain) {
                paths.retain(|_, r| r.tunnel_name != tunnel_name);
                if paths.is_empty() {
                    map.remove(domain);
                }
            }
        }
    }

    pub fn lookup(&self, normalized_host: &str, path: &str) -> Option<Arc<HttpRoute>> {
        let map = self.routes.read().unwrap_or_else(|e| e.into_inner());
        match_location(map.get(normalized_host)?, path.as_bytes()).cloned()
    }
}

fn match_location<'a>(paths: &'a Locations, mut probe: &[u8]) -> Option<&'a Arc<HttpRoute>> {
    loop {
        let (key, route) = paths
            .range::<[u8], _>((Unbounded, Included(probe)))
            .next_back()?;
        if probe.starts_with(key) {
            return Some(route);
        }
        // Every longer prefix of the request sorts strictly between this predecessor and the probe.
        let common = key.iter().zip(probe).take_while(|(a, b)| a == b).count();
        probe = &probe[..common];
    }
}

pub fn normalize_host(host: &str) -> String {
    let host = host.trim();
    let without_port = if let Some(h) = host.strip_prefix('[') {
        if let Some(end) = h.find(']') {
            &h[..end]
        } else {
            host
        }
    } else {
        host.split(':').next().unwrap_or(host)
    };
    without_port
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

pub fn build_domains(domains: &[String], root_domain: &str) -> anyhow::Result<Vec<String>> {
    let root = normalize_host(root_domain);
    let entries: Vec<String> = domains
        .iter()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .collect();

    if entries.is_empty() {
        return Err(anyhow::anyhow!(
            "http/https requires at least one domain (fullDomain or subdomain prefix)"
        ));
    }

    let mut out = Vec::new();
    for entry in &entries {
        out.push(expand_domain_entry(entry, &root)?);
    }

    let mut seen = std::collections::HashSet::new();
    out.retain(|d| seen.insert(d.clone()));
    Ok(out)
}

fn expand_domain_entry(entry: &str, root: &str) -> anyhow::Result<String> {
    let e = normalize_host(entry);
    if e.is_empty() {
        return Err(anyhow::anyhow!("empty domain entry"));
    }
    if e.starts_with('.') || e.ends_with('.') || e.contains("..") {
        return Err(anyhow::anyhow!("invalid domain entry: {entry:?}"));
    }

    if !e.contains('.') {
        if e.contains('*') {
            return Err(anyhow::anyhow!(
                "subdomain prefix must not contain '*': {entry:?}"
            ));
        }
        if e.len() > 63 {
            return Err(anyhow::anyhow!(
                "subdomain prefix too long (max 63): {entry:?}"
            ));
        }
        if root.is_empty() {
            return Err(anyhow::anyhow!(
                "domain prefix {entry:?} requires server rootDomain"
            ));
        }
        return Ok(normalize_host(&format!("{e}.{root}")));
    }

    if e.contains('*') {
        return Err(anyhow::anyhow!(
            "wildcard domains are not supported: {entry:?}"
        ));
    }

    if !root.is_empty() && is_host_under_root(&e, root) {
        return Ok(e);
    }

    Ok(e)
}

fn is_host_under_root(host: &str, root: &str) -> bool {
    host == root || host.ends_with(&format!(".{root}"))
}

pub fn expand_locations(locations: &[String]) -> Vec<String> {
    if locations.is_empty() {
        vec![String::new()]
    } else {
        locations.to_vec()
    }
}

pub fn parse_basic_auth(header_value: &str) -> Option<(String, String)> {
    const PREFIX: &str = "Basic ";
    let value = header_value.trim();
    if !value
        .as_bytes()
        .get(..PREFIX.len())?
        .eq_ignore_ascii_case(PREFIX.as_bytes())
    {
        return None;
    }
    let decoded = astrave_desktop::proto::base64::decode(value[PREFIX.len()..].trim().as_bytes())?;
    let text = String::from_utf8(decoded).ok()?;
    let (user, pass) = text.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

pub fn route_basic_auth_ok(
    route: &HttpRoute,
    is_proxy_request: bool,
    authorization: Option<&str>,
    proxy_authorization: Option<&str>,
) -> bool {
    if route.basic_auth_user.is_empty() && route.basic_auth_password.is_empty() {
        return true;
    }
    let creds = if is_proxy_request {
        let Some(h) = proxy_authorization else {
            return false;
        };
        parse_basic_auth(h)
    } else {
        let Some(h) = authorization else {
            return false;
        };
        parse_basic_auth(h)
    };
    match creds {
        Some((u, p)) => u == route.basic_auth_user && p == route.basic_auth_password,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(name: &str) -> HttpRoute {
        HttpRoute {
            tunnel_name: name.into(),
            control: Weak::new(),
            host_header_rewrite: String::new(),
            basic_auth_user: String::new(),
            basic_auth_password: String::new(),
            limiter: None,
        }
    }

    #[test]
    fn longest_prefix_and_atomic_conflicts() {
        let gw = HttpGw::new(80);
        let hosts = ["one.test".into(), "two.test".into()];
        let paths = ["", "/", "/a", "/a/", "/a/a", "/a/b", "/a/β", "/a/γ", "/z"];
        for path in paths {
            gw.register(&hosts, &[path.into()], route(path)).unwrap();
        }
        for query in [
            "", "/a", "/a/", "/a/_", "/a/α", "/a/γ123", "/a0", "/zz", "foo",
        ] {
            let expected = paths
                .iter()
                .filter(|p| query.starts_with(**p))
                .max_by_key(|p| p.len())
                .unwrap();
            assert_eq!(gw.lookup(&hosts[0], query).unwrap().tunnel_name, *expected);
        }
        assert!(Arc::ptr_eq(
            &gw.lookup(&hosts[0], "/a").unwrap(),
            &gw.lookup(&hosts[1], "/a").unwrap()
        ));
        assert!(
            gw.register(&hosts, &["/new".into(), "/a".into()], route("other"))
                .is_err()
        );
        assert_eq!(gw.lookup(&hosts[1], "/new").unwrap().tunnel_name, "/");
        gw.unregister_tunnel(&hosts, "/a/");
        assert_eq!(gw.lookup(&hosts[0], "/a/_").unwrap().tunnel_name, "/a");
        assert!(parse_basic_auth("Basicé").is_none());
    }
}
