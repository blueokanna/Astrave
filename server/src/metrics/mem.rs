use super::counter::Counter;
use super::history::{Bytes, History, day_key, hour_start};
use super::{RESERVE_DAYS, RESERVE_HOURS};
use jiff::{Timestamp, ToSpan, Zoned};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficWindow {
    Days7,
    Hours24,
}
impl TrafficWindow {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "24h" | "hour" | "hours" | "1d" => Self::Hours24,
            _ => Self::Days7,
        }
    }
}
#[derive(Debug, Clone)]
pub struct ServerSnapshot {
    pub total_traffic_in: u64,
    pub total_traffic_out: u64,
    pub active_connections: usize,
    pub tunnel_type_counts: HashMap<String, usize>,
}
#[derive(Debug, Clone)]
pub struct TunnelSnapshot {
    pub today_traffic_in: u64,
    pub today_traffic_out: u64,
    pub active_connections: usize,
    pub last_start_at: Option<i64>,
}
#[derive(Debug, Clone)]
pub struct TrafficPoint {
    pub date: String,
    pub traffic_in: u64,
    pub traffic_out: u64,
}
#[derive(Debug, Clone)]
pub struct TunnelTrafficHistory {
    pub name: String,
    pub unit: &'static str,
    pub granularity: &'static str,
    pub history: Vec<TrafficPoint>,
}
struct TunnelStats {
    tunnel_type: String,
    history: Option<Box<History>>,
    active_connections: Counter,
    last_start_unix: Option<i64>,
    closed_at: Option<Instant>,
}
impl TunnelStats {
    fn new(tunnel_type: &str) -> Self {
        Self {
            tunnel_type: tunnel_type.to_string(),
            history: None,
            active_connections: Counter::new(),
            last_start_unix: None,
            closed_at: None,
        }
    }
}
struct State {
    history: History,
    active_connections: Counter,
    closed_tunnels: BTreeSet<(Instant, String)>,
    tunnel_type_counts: HashMap<String, Counter>,
    tunnels: HashMap<String, TunnelStats>,
}
pub struct MemMetrics {
    state: Mutex<State>,
}
impl MemMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                history: History::default(),
                active_connections: Counter::new(),
                closed_tunnels: BTreeSet::new(),
                tunnel_type_counts: HashMap::new(),
                tunnels: HashMap::new(),
            }),
        })
    }
    pub fn server_snapshot(&self) -> ServerSnapshot {
        let day = day_key(&Zoned::now());
        let g = self.state.lock().expect("metrics lock");
        let traffic = g.history.days.get(day);
        let mut tunnel_type_counts = HashMap::new();
        for (k, v) in &g.tunnel_type_counts {
            let n = v.count().max(0) as usize;
            if n > 0 {
                tunnel_type_counts.insert(k.clone(), n);
            }
        }
        ServerSnapshot {
            total_traffic_in: traffic.inbound,
            total_traffic_out: traffic.outbound,
            active_connections: g.active_connections.count().max(0) as usize,
            tunnel_type_counts,
        }
    }
    pub fn tunnel_snapshot(&self, name: &str) -> Option<TunnelSnapshot> {
        let day = day_key(&Zoned::now());
        let g = self.state.lock().expect("metrics lock");
        g.tunnels.get(name).map(|p| {
            let bytes = p
                .history
                .as_ref()
                .map(|h| h.days.get(day))
                .unwrap_or_default();
            TunnelSnapshot {
                today_traffic_in: bytes.inbound,
                today_traffic_out: bytes.outbound,
                active_connections: p.active_connections.count().max(0) as usize,
                last_start_at: p.last_start_unix,
            }
        })
    }
    pub fn active_for<'a>(&self, names: impl Iterator<Item = &'a str>) -> usize {
        let state = self.state.lock().expect("metrics lock");
        names
            .filter_map(|name| state.tunnels.get(name))
            .map(|t| t.active_connections.count().max(0) as usize)
            .sum()
    }
    pub fn tunnel_traffic(
        &self,
        name: &str,
        window: TrafficWindow,
    ) -> Option<TunnelTrafficHistory> {
        let now = Zoned::now();
        let samples = {
            let state = self.state.lock().expect("metrics lock");
            Samples::read(state.tunnels.get(name)?.history.as_deref(), window, &now)
        };
        Some(samples.format(name, &now))
    }
    pub fn server_traffic(&self, window: TrafficWindow) -> TunnelTrafficHistory {
        let now = Zoned::now();
        let samples = {
            let state = self.state.lock().expect("metrics lock");
            Samples::read(Some(&state.history), window, &now)
        };
        samples.format("server", &now)
    }
    pub fn track_connection(
        self: &Arc<Self>,
        name: impl Into<String>,
        tunnel_type: impl Into<String>,
    ) -> ConnGuard {
        let name = name.into();
        let tunnel_type = tunnel_type.into();
        self.open_connection(&name, &tunnel_type);
        ConnGuard {
            metrics: Arc::clone(self),
            name,
            tunnel_type,
        }
    }
    pub fn new_tunnel(&self, name: &str, tunnel_type: &str) {
        let mut g = self.state.lock().expect("metrics lock");
        g.tunnel_type_counts
            .entry(tunnel_type.to_string())
            .or_default()
            .inc(1);

        let now_unix = Timestamp::now().as_second();
        let entry = g
            .tunnels
            .entry(name.to_string())
            .or_insert_with(|| TunnelStats::new(tunnel_type));

        let previous_close = entry.closed_at.take();
        if entry.tunnel_type != tunnel_type {
            *entry = TunnelStats::new(tunnel_type);
        }
        entry.last_start_unix = Some(now_unix);
        if let Some(at) = previous_close {
            g.closed_tunnels.remove(&(at, name.to_owned()));
        }
    }
    pub fn close_tunnel(&self, name: &str, tunnel_type: &str) {
        let mut g = self.state.lock().expect("metrics lock");
        if let Some(counter) = g.tunnel_type_counts.get(tunnel_type) {
            counter.dec(1);
        }
        if let Some(entry) = g.tunnels.get_mut(name)
            && entry.closed_at.is_none()
        {
            let at = Instant::now();
            entry.closed_at = Some(at);
            g.closed_tunnels.insert((at, name.to_owned()));
            const MAX_CLOSED: usize = 1024;
            if g.closed_tunnels.len() > MAX_CLOSED
                && let Some((_, expired)) = g.closed_tunnels.pop_first()
            {
                g.tunnels.remove(&expired);
            }
        }
    }
    pub fn open_connection(&self, name: &str, _tunnel_type: &str) {
        let mut g = self.state.lock().expect("metrics lock");
        g.active_connections.inc(1);
        if let Some(p) = g.tunnels.get_mut(name) {
            p.active_connections.inc(1);
        }
    }
    pub fn close_connection(&self, name: &str, _tunnel_type: &str) {
        let mut g = self.state.lock().expect("metrics lock");
        g.active_connections.dec(1);
        if let Some(p) = g.tunnels.get_mut(name) {
            p.active_connections.dec(1);
        }
    }
    pub fn record_traffic(&self, name: &str, inbound: u64, outbound: u64) {
        if inbound == 0 && outbound == 0 {
            return;
        }
        let now = Zoned::now();
        let day = day_key(&now);
        let hour = hour_start(&now).div_euclid(3600);
        let bytes = Bytes { inbound, outbound };
        let mut g = self.state.lock().expect("metrics lock");
        g.history.add(day, hour, bytes);
        if let Some(p) = g.tunnels.get_mut(name) {
            p.history
                .get_or_insert_with(Default::default)
                .add(day, hour, bytes);
        }
    }
}
pub struct ConnGuard {
    metrics: Arc<MemMetrics>,
    name: String,
    tunnel_type: String,
}
impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.metrics.close_connection(&self.name, &self.tunnel_type);
    }
}
// This request-local value leaves the mutex before formatting. Keeping the
// larger snapshot inline avoids an allocation on every hourly chart request.
#[expect(
    clippy::large_enum_variant,
    reason = "bounded stack storage for up to 24 pairs of byte counters"
)]
enum Samples {
    Days([Bytes; RESERVE_DAYS]),
    Hours([Bytes; RESERVE_HOURS]),
}
impl Samples {
    fn read(h: Option<&History>, window: TrafficWindow, now: &Zoned) -> Self {
        match window {
            TrafficWindow::Days7 => {
                Self::Days(h.map(|h| h.days.snapshot(day_key(now))).unwrap_or_default())
            }
            TrafficWindow::Hours24 => Self::Hours(
                h.map(|h| h.hours.snapshot(hour_start(now).div_euclid(3600)))
                    .unwrap_or_default(),
            ),
        }
    }
    fn format(&self, name: &str, now: &Zoned) -> TunnelTrafficHistory {
        let (granularity, samples): (_, &[Bytes]) = match self {
            Self::Days(s) => ("day", s),
            Self::Hours(s) => ("hour", s),
        };
        let hour = hour_start(now);
        let history = samples
            .iter()
            .enumerate()
            .rev()
            .map(|(age, b)| {
                let date = match self {
                    Self::Days(_) => (now.date() - (age as i64).days()).to_string(),
                    Self::Hours(_) => Timestamp::from_second(hour - 3600 * age as i64)
                        .expect("recent hour")
                        .to_zoned(now.time_zone().clone())
                        .strftime("%Y-%m-%dT%H:00")
                        .to_string(),
                };
                TrafficPoint {
                    date,
                    traffic_in: b.inbound,
                    traffic_out: b.outbound,
                }
            })
            .collect();
        TunnelTrafficHistory {
            name: name.to_string(),
            unit: "bytes",
            granularity,
            history,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tunnels_only_allocate_history_on_nonzero_traffic() {
        let metrics = MemMetrics::new();
        metrics.new_tunnel("one", "tcp");
        metrics.record_traffic("one", 0, 0);
        let empty = metrics.tunnel_traffic("one", TrafficWindow::Days7).unwrap();
        assert_eq!(empty.history.len(), RESERVE_DAYS);
        assert!(
            empty
                .history
                .iter()
                .all(|p| p.traffic_in == 0 && p.traffic_out == 0)
        );
        assert!(
            metrics.state.lock().unwrap().tunnels["one"]
                .history
                .is_none()
        );
        metrics.record_traffic("one", u64::MAX, 11);
        metrics.record_traffic("one", 1, 7);
        let tunnel = metrics.tunnel_snapshot("one").unwrap();
        assert_eq!(
            (tunnel.today_traffic_in, tunnel.today_traffic_out),
            (u64::MAX, 18)
        );
        for window in [TrafficWindow::Days7, TrafficWindow::Hours24] {
            let history = metrics.server_traffic(window);
            let last = history.history.last().unwrap();
            assert_eq!((last.traffic_in, last.traffic_out), (u64::MAX, 18));
        }
    }
    #[test]
    fn history_eviction_preserves_reopened_tunnels() {
        let m = MemMetrics::new();
        for name in ["keep", "first"] {
            m.new_tunnel(name, "tcp");
            m.close_tunnel(name, "tcp");
        }
        m.new_tunnel("keep", "tcp");
        for i in 0..1024 {
            let name = i.to_string();
            m.new_tunnel(&name, "tcp");
            m.close_tunnel(&name, "tcp");
        }
        assert!(m.tunnel_snapshot("first").is_none());
        assert!(m.tunnel_snapshot("keep").is_some());
        assert!(m.tunnel_snapshot("1023").is_some());
    }
}
