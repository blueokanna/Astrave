use super::model::{ClientInfo, Page, SystemStatus, TunnelInfo};
use crate::{control::Control, service::Service, tunnel::TunnelOwner};
use std::sync::Arc;

impl Service {
    pub async fn dashboard_status(&self) -> SystemStatus {
        let (online, total) = {
            let map = self.controls.lock().await;
            (
                map.len(),
                map.len() + self.agents.offline_count(|id| map.contains_key(id)),
            )
        };
        let stats = self.metrics.server_snapshot();
        SystemStatus {
            client_counts: online,
            total_client_counts: total,
            tunnel_type_count: stats.tunnel_type_counts.into_iter().collect(),
            active_connections: stats.active_connections,
            total_traffic_in: stats.total_traffic_in,
            total_traffic_out: stats.total_traffic_out,
        }
    }

    pub async fn dashboard_clients(
        &self,
        id: Option<&str>,
        status: &str,
        page: usize,
        page_size: usize,
    ) -> Page<ClientInfo> {
        let (mut online, mut offline) = {
            let map = self.controls.lock().await;
            let online = if status == "offline" {
                Vec::new()
            } else {
                map.values()
                    .map(|e| &e.control)
                    .filter(|c| id.is_none_or(|id| c.session_id == id || c.agent_id == id))
                    .map(Arc::clone)
                    .collect::<Vec<_>>()
            };
            let mut offline = if status == "online" {
                Vec::new()
            } else {
                self.agents.offline_matching(id)
            };
            offline.retain(|e| !map.contains_key(&e.session_id));
            (online, offline)
        };
        fn key<'a>(agent: &'a str, session: &'a str) -> (&'a str, &'a str) {
            (if agent.is_empty() { session } else { agent }, session)
        }
        online.sort_unstable_by(|a, b| {
            key(&a.agent_id, &a.session_id).cmp(&key(&b.agent_id, &b.session_id))
        });
        offline.sort_unstable_by(|a, b| {
            key(&a.agent_id, &a.session_id).cmp(&key(&b.agent_id, &b.session_id))
        });
        let total = online.len() + offline.len();
        let start = (page - 1).saturating_mul(page_size);
        let offline_start = start.saturating_sub(online.len());
        let mut items = Vec::new();
        for c in online.into_iter().skip(start).take(page_size) {
            items.push(online_info(&c).await);
        }
        let remaining = page_size - items.len();
        items.extend(
            offline
                .into_iter()
                .skip(offline_start)
                .take(remaining)
                .map(|c| ClientInfo {
                    session_id: c.session_id,
                    agent_id: c.agent_id,
                    user: c.user,
                    hostname: c.hostname,
                    os: c.os,
                    arch: c.arch,
                    client_ip: c.client_ip,
                    version: c.version,
                    tunnel_count: c.tunnel_count,
                    active_connections: 0,
                    connected_secs: c.disconnected_at.map_or(0, |t| t.elapsed().as_secs()),
                    status: "offline".into(),
                }),
        );
        Page {
            total,
            page,
            page_size,
            items,
        }
    }

    pub async fn dashboard_tunnel(&self, name: &str, owner: TunnelOwner) -> Option<TunnelInfo> {
        let control = {
            let map = self.controls.lock().await;
            let c = &map.get(&owner.session_id)?.control;
            if c.generation != owner.generation {
                return None;
            }
            Arc::clone(c)
        };
        let s = control.tunnel_summary(name).await?;
        let stats = self.metrics.tunnel_snapshot(name);
        Some(TunnelInfo {
            name: s.name,
            tunnel_type: s.tunnel_type,
            remote_addr: s.remote_addr,
            local_addr: s.local_addr,
            session_id: owner.session_id,
            status: s.status,
            today_traffic_in: stats.as_ref().map_or(0, |t| t.today_traffic_in),
            today_traffic_out: stats.as_ref().map_or(0, |t| t.today_traffic_out),
            active_connections: stats.as_ref().map_or(0, |t| t.active_connections),
            last_start_time: stats
                .and_then(|t| jiff::Timestamp::from_second(t.last_start_at?).ok())
                .map(|t| {
                    t.to_zoned_datetime(jiff::tz::TimeZone::system())
                        .strftime("%m-%d %H:%M:%S")
                        .to_string()
                }),
        })
    }

    pub async fn dashboard_tunnels(
        &self,
        session: &str,
        needle: &str,
        protocol: &str,
        page: usize,
        page_size: usize,
    ) -> Page<TunnelInfo> {
        let (total, owners) = self.tunnel_registry.page(
            session,
            needle,
            protocol,
            (page - 1).saturating_mul(page_size),
            page_size,
        );
        let mut items = Vec::with_capacity(owners.len());
        for (name, owner) in owners {
            if let Some(row) = self.dashboard_tunnel(&name, owner).await {
                items.push(row);
            }
        }
        Page {
            total,
            page,
            page_size,
            items,
        }
    }
}

async fn online_info(c: &Control) -> ClientInfo {
    let (tunnel_count, active_connections) = c.dashboard_counts().await;
    ClientInfo {
        session_id: c.session_id.clone(),
        agent_id: c.agent_id.clone(),
        user: c.user.clone(),
        hostname: c.hostname.clone(),
        os: c.os.clone(),
        arch: c.arch.clone(),
        client_ip: c.client_ip.clone(),
        version: c.version.clone(),
        tunnel_count,
        active_connections,
        connected_secs: c.connected_at.elapsed().as_secs(),
        status: "online".into(),
    }
}
