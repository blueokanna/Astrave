use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;
use std::time::Instant;

pub const MAX_AGENT_ID_LEN: usize = 64;
pub const MAX_SESSION_ID_LEN: usize = 64;
pub const MAX_USER_LEN: usize = 64;
const MAX_OFFLINE_AGENTS: usize = 1024;

#[derive(Debug, Clone)]
pub struct AgentEntry {
    pub user: String,
    pub agent_id: String,
    pub session_id: String,
    pub generation: u64,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub client_ip: String,
    pub version: String,
    pub tunnel_count: usize,
    pub online: bool,
    pub disconnected_at: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct AgentOnlineSpec {
    pub user: String,
    pub agent_id: String,
    pub session_id: String,
    pub generation: u64,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub client_ip: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRegisterError {
    Conflict,
}

#[derive(Debug, Default)]
pub struct AgentRegistry {
    by_key: Mutex<HashMap<String, AgentEntry>>,
    by_session: Mutex<HashMap<String, String>>,
    // Acquire after by_key, and after by_session when updating a session.
    offline: Mutex<BTreeSet<(Instant, String)>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_online(&self, spec: AgentOnlineSpec) -> Result<String, AgentRegisterError> {
        let now = Instant::now();
        let explicit = !spec.agent_id.is_empty();
        let effective_id = if explicit {
            spec.agent_id.as_str()
        } else {
            spec.session_id.as_str()
        };
        let key = compose_key(&spec.user, effective_id);

        let mut by_key = self.by_key.lock().unwrap_or_else(|e| e.into_inner());
        let mut by_session = self.by_session.lock().unwrap_or_else(|e| e.into_inner());
        let mut offline = self.offline.lock().unwrap_or_else(|e| e.into_inner());

        if explicit
            && let Some(existing) = by_key.get(&key)
            && existing.online
            && !existing.session_id.is_empty()
            && existing.session_id != spec.session_id
        {
            return Err(AgentRegisterError::Conflict);
        }

        if let Some(prev_key) = by_session.get(&spec.session_id).cloned()
            && prev_key != key
        {
            if let Some(prev) = by_key.get_mut(&prev_key)
                && prev.session_id == spec.session_id
            {
                if prev.agent_id.is_empty() {
                    by_key.remove(&prev_key);
                } else {
                    let count = prev.tunnel_count;
                    set_offline(prev, now, count);
                    offline.insert((now, prev_key.clone()));
                }
            }
            by_session.remove(&spec.session_id);
        }

        match by_key.get_mut(&key) {
            Some(entry) => {
                if entry.online
                    && !entry.session_id.is_empty()
                    && entry.session_id != spec.session_id
                {
                    by_session.remove(&entry.session_id);
                }
                entry.user = spec.user;
                entry.agent_id = spec.agent_id;
                entry.session_id = spec.session_id.clone();
                entry.generation = spec.generation;
                entry.hostname = spec.hostname;
                entry.os = spec.os;
                entry.arch = spec.arch;
                entry.client_ip = spec.client_ip;
                entry.version = spec.version;
                entry.online = true;
                if let Some(at) = entry.disconnected_at.take() {
                    offline.remove(&(at, key.clone()));
                }
            }
            None => {
                by_key.insert(
                    key.clone(),
                    AgentEntry {
                        user: spec.user,
                        agent_id: spec.agent_id,
                        session_id: spec.session_id.clone(),
                        generation: spec.generation,
                        hostname: spec.hostname,
                        os: spec.os,
                        arch: spec.arch,
                        client_ip: spec.client_ip,
                        version: spec.version,
                        tunnel_count: 0,
                        online: true,
                        disconnected_at: None,
                    },
                );
            }
        }

        by_session.insert(spec.session_id, key.clone());
        trim_offline(&mut by_key, &mut offline);
        Ok(key)
    }

    pub fn release(&self, session_id: &str, generation: u64, tunnel_count: usize) {
        let now = Instant::now();
        let mut by_key = self.by_key.lock().unwrap_or_else(|e| e.into_inner());
        let mut by_session = self.by_session.lock().unwrap_or_else(|e| e.into_inner());

        let Some(key) = by_session.get(session_id).cloned() else {
            return;
        };
        let Some(entry) = by_key.get_mut(&key) else {
            by_session.remove(session_id);
            return;
        };
        if entry.session_id != session_id || entry.generation != generation {
            return;
        }

        by_session.remove(session_id);
        if entry.agent_id.is_empty() {
            by_key.remove(&key);
        } else {
            set_offline(entry, now, tunnel_count);
            let mut offline = self.offline.lock().unwrap_or_else(|e| e.into_inner());
            offline.insert((now, key));
            trim_offline(&mut by_key, &mut offline);
        }
    }

    pub fn offline_matching(&self, id: Option<&str>) -> Vec<AgentEntry> {
        let entries = self.by_key.lock().unwrap_or_else(|e| e.into_inner());
        self.offline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter_map(|(_, key)| entries.get(key))
            .filter(|entry| {
                !entry.online && id.is_none_or(|id| id == entry.session_id || id == entry.agent_id)
            })
            .cloned()
            .collect()
    }

    pub fn offline_count(&self, is_online: impl Fn(&str) -> bool) -> usize {
        let entries = self.by_key.lock().unwrap_or_else(|e| e.into_inner());
        self.offline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter_map(|(_, key)| entries.get(key))
            .filter(|e| !e.online && !is_online(&e.session_id))
            .count()
    }
}

fn trim_offline(
    entries: &mut HashMap<String, AgentEntry>,
    offline: &mut BTreeSet<(Instant, String)>,
) {
    if offline.len() > MAX_OFFLINE_AGENTS
        && let Some((_, key)) = offline.pop_first()
    {
        entries.remove(&key);
    }
}

fn set_offline(entry: &mut AgentEntry, now: Instant, tunnel_count: usize) {
    entry.generation = 0;
    entry.online = false;
    entry.tunnel_count = tunnel_count;
    entry.disconnected_at = Some(now);
}

fn compose_key(user: &str, id: &str) -> String {
    format!("{}:{user}{id}", user.len())
}

pub fn sanitize_wire_id(raw: &str, max_len: usize) -> Result<String, &'static str> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(String::new());
    }
    if s.len() > max_len {
        return Err("identifier too long");
    }
    if s.chars().any(|c| c.is_control()) {
        return Err("identifier contains control characters");
    }
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconnect_and_trim_keep_current_session_mapping() {
        fn connect(registry: &AgentRegistry, agent: &str, session: &str, generation: u64) {
            registry
                .try_online(AgentOnlineSpec {
                    user: String::new(),
                    agent_id: agent.into(),
                    session_id: session.into(),
                    generation,
                    hostname: String::new(),
                    os: String::new(),
                    arch: String::new(),
                    client_ip: String::new(),
                    version: String::new(),
                })
                .unwrap();
        }
        let r = AgentRegistry::new();
        connect(&r, "a", "x", 1);
        r.release("x", 1, 2);
        connect(&r, "b", "x", 2);
        connect(&r, "a", "y", 3);
        r.release("x", 2, 0);
        assert_eq!(r.offline_matching(Some("b")).len(), 1);
        for i in 0..MAX_OFFLINE_AGENTS {
            let s = i.to_string();
            connect(&r, &s, &s, 4);
            r.release(&s, 4, 1);
        }
        assert_eq!(r.offline_matching(None).len(), MAX_OFFLINE_AGENTS);
        assert!(r.offline_matching(Some("b")).is_empty());
        assert_ne!(compose_key("a.b", "c"), compose_key("a", "b.c"));
    }
}
