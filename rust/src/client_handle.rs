use crate::client::local_control;
use crate::client::reload::ReloadOutcome;
use crate::client::service::{ReloadRequest, Service};
use crate::core::config::ClientConfig;
use crate::core::transport::Protocol;
use anyhow::{Result, anyhow, bail};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

const UDP_SESSION_DRAIN: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientStatus {
    Stopped,
    Starting,
    Running,
    Reconnecting,
    Stopping,
}

impl ClientStatus {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running | Self::Reconnecting | Self::Stopping
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct StartOptions {
    pub local_control: bool,
}

#[derive(Debug, Default)]
struct TunnelRemoteState {
    generation: u64,
    by_name: std::collections::HashMap<String, String>,
}

struct Inner {
    run: Mutex<Option<ActiveRun>>,
    last_error: Mutex<Option<String>>,
    pending_logs: Mutex<std::collections::VecDeque<String>>,
    tunnel_remotes: Mutex<TunnelRemoteState>,
    reload_lock: tokio::sync::Mutex<()>,
}

struct ActiveRun {
    status: ClientStatus,
    cloudflare: bool,
    cancel: CancellationToken,
    finished: CancellationToken,
    abort: AbortHandle,
    reload_tx: mpsc::Sender<ReloadRequest>,
}

// The service task owns cleanup even if it is aborted before its first poll.
struct RunGuard(ClientHandle);

impl Drop for RunGuard {
    fn drop(&mut self) {
        let mut slot = self.0.inner.run.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = slot.take() {
            run.cancel.cancel();
            self.0.clear_tunnel_remotes();
            run.finished.cancel();
        }
    }
}

#[derive(Clone)]
pub struct ClientHandle {
    inner: Arc<Inner>,
}

impl Default for ClientHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                run: Mutex::new(None),
                last_error: Mutex::new(None),
                pending_logs: Mutex::default(),
                tunnel_remotes: Mutex::new(TunnelRemoteState::default()),
                reload_lock: tokio::sync::Mutex::new(()),
            }),
        }
    }

    pub fn status(&self) -> ClientStatus {
        self.inner
            .run
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or(ClientStatus::Stopped, |run| run.status)
    }

    pub fn last_error(&self) -> Option<String> {
        self.inner
            .last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn take_last_error(&self) -> Option<String> {
        self.inner
            .last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    pub fn push_log(&self, line: impl Into<String>) {
        self.enqueue_log(line.into());
    }

    pub fn drain_logs(&self) -> Vec<String> {
        std::mem::take(
            &mut *self
                .inner
                .pending_logs
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        )
        .into()
    }

    pub fn tunnel_remotes_if_changed(
        &self,
        since_gen: u64,
    ) -> Option<(u64, std::collections::HashMap<String, String>)> {
        let g = self
            .inner
            .tunnel_remotes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if g.generation == since_gen {
            return None;
        }
        Some((g.generation, g.by_name.clone()))
    }

    pub fn clear_tunnel_remotes(&self) {
        let mut g = self
            .inner
            .tunnel_remotes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        g.by_name.clear();
        g.generation = g.generation.wrapping_add(1);
    }

    pub(crate) fn set_tunnel_remote(&self, name: String, remote_addr: String) {
        if name.is_empty() {
            return;
        }
        let mut g = self
            .inner
            .tunnel_remotes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if g.by_name.get(&name) == Some(&remote_addr) {
            return;
        }
        g.by_name.insert(name, remote_addr);
        g.generation = g.generation.wrapping_add(1);
    }

    fn remove_tunnel_remote(&self, name: String) {
        if name.is_empty() {
            return;
        }
        let mut g = self
            .inner
            .tunnel_remotes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if g.by_name.remove(&name).is_some() {
            g.generation = g.generation.wrapping_add(1);
        }
    }

    fn enqueue_log(&self, line: String) {
        if let Ok(mut g) = self.inner.pending_logs.lock() {
            const MAX_PENDING: usize = 500;
            if g.len() >= MAX_PENDING {
                g.pop_front();
            }
            g.push_back(line);
        }
    }

    pub(crate) fn set_status(&self, s: ClientStatus) {
        let mut slot = self.inner.run.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = slot.as_mut()
            && run.status != ClientStatus::Stopping
        {
            run.status = s;
        }
    }

    pub(crate) fn set_error(&self, e: Option<String>) {
        if let Ok(mut g) = self.inner.last_error.lock() {
            *g = e;
        }
    }

    pub async fn run_foreground(
        self,
        cfg: ClientConfig,
        config_path: PathBuf,
        opts: StartOptions,
    ) -> Result<()> {
        let join = self.spawn_service(cfg, config_path, opts, true)?;
        tokio_util::task::AbortOnDropHandle::new(join).await?
    }

    pub fn start_with(
        &self,
        cfg: ClientConfig,
        config_path: PathBuf,
        opts: StartOptions,
    ) -> Result<()> {
        self.spawn_service(cfg, config_path, opts, false)?;
        Ok(())
    }

    fn spawn_service(
        &self,
        mut cfg: ClientConfig,
        config_path: PathBuf,
        opts: StartOptions,
        is_foreground: bool,
    ) -> Result<JoinHandle<Result<()>>> {
        cfg.prepare_runtime(&config_path);
        cfg.validate()?;
        let runtime = tokio::runtime::Handle::try_current()?;
        let mut slot = self.inner.run.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            bail!("client already running");
        }
        let cancel = CancellationToken::new();
        let (reload_tx, reload_rx) = mpsc::channel(4);
        let cloudflare = cfg.cloudflare.is_some();
        self.set_error(None);
        self.clear_tunnel_remotes();
        let (begin, ready) = oneshot::channel::<RunGuard>();
        let service_cancel = cancel.clone();
        let join = runtime.spawn(async move {
            let guard = ready.await?;
            let result = guard
                .0
                .run_inner(cfg, opts, is_foreground, service_cancel, reload_rx)
                .await;
            if let Err(e) = &result {
                tracing::error!(error = %e, "client service ended with error");
                guard.0.set_error(Some(e.to_string()));
            }
            result
        });
        *slot = Some(ActiveRun {
            status: ClientStatus::Starting,
            cloudflare,
            cancel,
            finished: CancellationToken::new(),
            abort: join.abort_handle(),
            reload_tx,
        });
        drop(slot);
        begin
            .send(RunGuard(self.clone()))
            .map_err(|_| anyhow!("client runtime shut down during start"))?;
        Ok(join)
    }

    pub fn start(&self, cfg: ClientConfig, config_path: PathBuf) -> Result<()> {
        self.start_with(cfg, config_path, StartOptions::default())
    }

    async fn run_inner(
        &self,
        cfg: ClientConfig,
        opts: StartOptions,
        is_foreground: bool,
        cancel: CancellationToken,
        mut reload_rx: mpsc::Receiver<ReloadRequest>,
    ) -> Result<()> {
        let drain_udp = is_foreground && needs_udp_session_drain(&cfg);
        let mut helpers = tokio::task::JoinSet::new();
        if is_foreground && (cfg.cloudflare.is_some() || drain_udp) {
            let token = cancel.clone();
            helpers.spawn(async move {
                tokio::select! {
                    _ = token.cancelled() => {},
                    received = wait_termination_signal() => if received { token.cancel(); },
                }
            });
        }

        if opts.local_control {
            let socket_path = local_control::default_socket_path();
            let hc = self.clone();
            let cc = cancel.clone();
            helpers.spawn(async move {
                if let Err(e) = local_control::serve(socket_path, hc, cc).await {
                    tracing::warn!(error = %e, "local control socket stopped");
                }
            });
        }

        let on_status = {
            let h = self.clone();
            move |st| h.set_status(st)
        };
        let on_log = {
            let h = self.clone();
            move |line: String| h.enqueue_log(line)
        };
        let on_tunnel_remote: Arc<dyn Fn(String, String) + Send + Sync> = {
            let h = self.clone();
            Arc::new(move |name, remote| h.set_tunnel_remote(name, remote))
        };
        let on_tunnel_removed: Arc<dyn Fn(String) + Send + Sync> = {
            let h = self.clone();
            Arc::new(move |name| h.remove_tunnel_remote(name))
        };
        let on_remotes_clear: Arc<dyn Fn() + Send + Sync> = {
            let h = self.clone();
            Arc::new(move || h.clear_tunnel_remotes())
        };

        let result = if cfg.cloudflare.is_some() {
            crate::cloudflare::run_service(cfg, self.clone(), cancel.clone(), &mut reload_rx).await
        } else {
            let shared_cfg = Arc::new(RwLock::new(cfg.clone()));
            Service::new(cfg)
                .run(
                    cancel.clone(),
                    &mut reload_rx,
                    shared_cfg,
                    on_status,
                    on_log,
                    on_tunnel_remote,
                    on_tunnel_removed,
                    on_remotes_clear,
                )
                .await
        };

        cancel.cancel();
        while let Some(joined) = helpers.join_next().await {
            if let Err(error) = joined {
                tracing::warn!(%error, "client helper stopped");
            }
        }
        if drain_udp {
            sleep(UDP_SESSION_DRAIN).await;
        }
        result
    }

    pub async fn reload(&self, cfg: ClientConfig, config_path: PathBuf) -> Result<ReloadOutcome> {
        let tx = {
            let slot = self.inner.run.lock().unwrap_or_else(|e| e.into_inner());
            let run = slot
                .as_ref()
                .ok_or_else(|| anyhow!("client is not running"))?;
            if run.status == ClientStatus::Stopping {
                bail!("client is stopping");
            }
            if run.cloudflare != cfg.cloudflare.is_some() {
                bail!("Disconnect before changing connector mode");
            }
            run.reload_tx.clone()
        };
        let _guard = self.inner.reload_lock.lock().await;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(ReloadRequest {
            cfg,
            config_path,
            reply: reply_tx,
        })
        .await
        .map_err(|_| anyhow!("reload channel closed"))?;
        match tokio::time::timeout(Duration::from_secs(120), reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(anyhow!("reload response channel closed")),
            Err(_) => Err(anyhow!("reload timed out after 120s")),
        }
    }

    pub async fn reload_from_path(&self, path: &Path) -> Result<ReloadOutcome> {
        let cfg = ClientConfig::load(path)?;
        self.reload(cfg, path.to_path_buf()).await
    }

    pub async fn stop(&self) {
        let (finished, abort) = {
            let mut slot = self.inner.run.lock().unwrap_or_else(|e| e.into_inner());
            let Some(run) = slot.as_mut() else { return };
            run.status = ClientStatus::Stopping;
            self.set_error(None);
            run.cancel.cancel();
            (run.finished.clone(), run.abort.clone())
        };
        if tokio::time::timeout(Duration::from_secs(5), finished.cancelled())
            .await
            .is_err()
        {
            tracing::warn!("client stop timed out after 5s, aborting task");
            abort.abort();
            finished.cancelled().await;
        }
    }
}

fn needs_udp_session_drain(cfg: &ClientConfig) -> bool {
    matches!(cfg.protocol().ok(), Some(Protocol::Quic | Protocol::Kcp))
}

async fn wait_termination_signal() -> bool {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "failed to install SIGTERM handler");
                return false;
            }
        };
        tokio::select! {
            res = tokio::signal::ctrl_c() => {
                if let Err(e) = res {
                    tracing::warn!(error = %e, "failed to wait for Ctrl-C");
                    return false;
                }
                true
            }
            _ = sigterm.recv() => true,
        }
    }
    #[cfg(not(unix))]
    {
        match tokio::signal::ctrl_c().await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, "failed to wait for Ctrl-C");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn overlapping_stops_wait_for_the_same_run_before_restart() {
        let path = PathBuf::from("agent.toml");
        let cfg: ClientConfig = crate::core::config::parse_config_toml(
            "server='127.0.0.1:1'\ntransport.tls.enable=false",
            &path,
        )
        .unwrap();
        let handle = ClientHandle::new();
        handle.start(cfg.clone(), path.clone()).unwrap();
        let first = handle.stop();
        let second = handle.stop();
        tokio::pin!(first, second);
        assert!(futures_util::poll!(&mut first).is_pending());
        assert!(futures_util::poll!(&mut second).is_pending());
        assert_eq!(handle.status(), ClientStatus::Stopping);
        assert!(handle.start(cfg.clone(), path.clone()).is_err());
        tokio::join!(first, second);
        assert_eq!(handle.status(), ClientStatus::Stopped);
        assert!(handle.last_error().is_none());
        handle.start(cfg.clone(), path.clone()).unwrap();
        handle.reload(cfg, path).await.unwrap();
        handle.stop().await;
        assert_eq!(handle.status(), ClientStatus::Stopped);
    }
}
