use anyhow::{Result, anyhow};
use std::collections::BTreeMap;
use std::sync::Mutex;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelOwner {
    pub session_id: String,
    pub generation: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_only_published_names_and_checks_ownership() {
        let registry = TunnelRegistry::new();
        let first = TunnelOwner {
            session_id: "one".into(),
            generation: 1,
        };
        let stale = TunnelOwner {
            session_id: "one".into(),
            generation: 0,
        };
        for name in ["beta", "alpha", "gamma", "pending"] {
            registry.try_insert(name, first.clone()).unwrap();
            if name != "pending" {
                registry.publish(name, &first, "tcp");
            }
        }
        registry.remove_if_owner("beta", &stale);
        let (total, items) = registry.page("one", "A", "tcp", 1, 1);
        assert_eq!(total, 3);
        assert_eq!(items, vec![("beta".into(), first)]);
        assert_eq!(registry.page("one", "", "udp", 0, 20).0, 0);
        assert!(registry.owner("pending").is_none());
    }
}
#[derive(Debug)]
struct Registration {
    owner: TunnelOwner,
    published: Option<(&'static str, String)>,
}
#[derive(Debug, Default)]
pub struct TunnelRegistry {
    by_name: Mutex<BTreeMap<String, Registration>>,
}
impl TunnelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_insert(&self, name: &str, owner: TunnelOwner) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow!("empty tunnel name"));
        }
        let mut map = self.by_name.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = map.get(name) {
            return Err(anyhow!(
                "tunnel `{name}` is already registered (session={}, generation={})",
                existing.owner.session_id,
                existing.owner.generation
            ));
        }
        map.insert(
            name.to_string(),
            Registration {
                owner,
                published: None,
            },
        );
        Ok(())
    }

    pub fn remove_if_owner(&self, name: &str, owner: &TunnelOwner) {
        let mut map = self.by_name.lock().unwrap_or_else(|e| e.into_inner());
        let name = name.trim();
        if map.get(name).is_some_and(|r| &r.owner == owner) {
            map.remove(name);
        }
    }

    pub fn owner(&self, name: &str) -> Option<TunnelOwner> {
        self.by_name
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name.trim())
            .filter(|r| r.published.is_some())
            .map(|r| r.owner.clone())
    }

    pub fn publish(&self, name: &str, owner: &TunnelOwner, protocol: &'static str) {
        if let Some(r) = self
            .by_name
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(name.trim())
            .filter(|r| &r.owner == owner)
        {
            r.published = Some((protocol, name.to_owned()));
        }
    }

    pub fn page(
        &self,
        session: &str,
        needle: &str,
        protocol: &str,
        offset: usize,
        limit: usize,
    ) -> (usize, Vec<(String, TunnelOwner)>) {
        let map = self.by_name.lock().unwrap_or_else(|e| e.into_inner());
        let mut total = 0;
        let mut rows = Vec::new();
        for r in map.values() {
            let Some((ty, name)) = &r.published else {
                continue;
            };
            if !protocol.is_empty() && protocol != *ty {
                continue;
            }
            let owner = &r.owner;
            if !session.is_empty() && owner.session_id != session {
                continue;
            }
            if !needle.is_empty()
                && !name
                    .as_bytes()
                    .windows(needle.len())
                    .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
            {
                continue;
            }
            if total >= offset && rows.len() < limit {
                rows.push((name.clone(), owner.clone()));
            }
            total += 1;
        }
        (total, rows)
    }
}
