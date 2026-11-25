//! Native Cloudflare Tunnel connector, embedded in the CLI and desktop runtime.
#[path = "cloudflare_config.rs"]
mod config;
#[path = "cloudflare_proxy.rs"]
mod proxy;
#[path = "cloudflare_routes.rs"]
mod routes;
pub use config::Config;
#[cfg(test)]
#[path = "cloudflare_tests.rs"]
mod tests;

use crate::client::{ClientHandle, ClientStatus};
use anyhow::{Result, bail};
use std::{
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub(crate) async fn run(
    config: Config,
    handle: ClientHandle,
    cancel: CancellationToken,
) -> Result<()> {
    crate::core::transport::install_ring_provider()?;
    let credentials = config.credentials()?;
    let transport = config.transport()?;
    let connected = Arc::new(Mutex::new(0u8));
    let proxy = proxy::Proxy {
        routes: Arc::new(RwLock::new(routes::Routes::empty())),
        cancel: cancel.child_token(),
        tasks: TaskTracker::new(),
    };
    let _cancel_on_drop = proxy.cancel.clone().drop_guard();
    let mut connections = tokio::task::JoinSet::new();
    handle.push_log(format!(
        "INFO  Cloudflare Tunnel {}: starting {} native connections",
        credentials.tunnel_identifier, config.connections
    ));
    for index in 0..config.connections {
        let state_handle = handle.clone();
        let count = connected.clone();
        let state_cancel = cancel.clone();
        let options = libcfd::EdgeOptions {
            transport,
            connection_index: index,
            region: (!config.region.is_empty()).then(|| config.region.clone()),
            grace_period: Duration::from_secs(1),
            maximum_quic_failures: 2,
            on_connection_state: Some(Arc::new(move |up| {
                let mut mask = count.lock().unwrap_or_else(|e| e.into_inner());
                let previous = *mask;
                let bit = 1 << index;
                if up {
                    *mask |= bit;
                } else {
                    *mask &= !bit;
                }
                if state_cancel.is_cancelled() || state_handle.status() == ClientStatus::Stopping {
                    return;
                }
                state_handle.set_status(if *mask != 0 {
                    ClientStatus::Running
                } else {
                    ClientStatus::Reconnecting
                });
                if previous != *mask {
                    state_handle.push_log(format!(
                        "INFO  Cloudflare connection {} {}; {} active",
                        index + 1,
                        if up { "registered" } else { "disconnected" },
                        mask.count_ones()
                    ));
                }
            })),
            apply_configuration: Some(configuration_handler(proxy.routes.clone(), handle.clone())),
            ..Default::default()
        };
        let tunnel = libcfd::Tunnel::Named(credentials.clone());
        let origin = libcfd::Origin::http(proxy.clone()).with_websocket(proxy.clone());
        let shutdown = cancel.clone();
        connections.spawn(async move {
            libcfd::EdgeConnector::new(options)
                .run(tunnel, origin, shutdown.cancelled_owned())
                .await
        });
    }
    let mut result = Ok(());
    while let Some(outcome) = connections.join_next().await {
        match outcome {
            Ok(Ok(())) if cancel.is_cancelled() => {}
            Ok(Ok(())) => {
                result = Err(anyhow::anyhow!("Cloudflare connector ended unexpectedly"));
                cancel.cancel();
            }
            Ok(Err(error)) => {
                result = Err(error.into());
                cancel.cancel();
            }
            Err(error) => {
                result = Err(error.into());
                cancel.cancel();
            }
        }
    }
    proxy.cancel.cancel();
    proxy.tasks.close();
    proxy.tasks.wait().await;
    result
}

fn configuration_handler(
    routes: Arc<RwLock<routes::Routes>>,
    handle: ClientHandle,
) -> libcfd::edge::ApplyConfiguration {
    Arc::new(move |version, bytes| {
        let applied = routes.read().unwrap_or_else(|e| e.into_inner()).version;
        if version <= applied {
            return Ok(applied);
        }
        // Compiling route expressions and loading TLS roots can take time. Keep serving
        // the current table while a new one is prepared by an edge connection.
        let parsed = routes::Routes::parse(version, bytes);
        let mut current = routes.write().unwrap_or_else(|e| e.into_inner());
        if version <= current.version {
            return Ok(current.version);
        }
        let next = match parsed {
            Ok(next) => next,
            Err(error) => {
                let message = format!("Cloudflare configuration rejected: {error}");
                handle.set_error(Some(message.clone()));
                tracing::error!("{message}");
                return Err((current.version, message));
            }
        };
        handle.clear_tunnel_remotes();
        for rule in &next.rules {
            if !rule.hostname.is_empty() {
                let path = rule.path.as_ref().map_or("", |p| p.as_str());
                handle.set_tunnel_remote(
                    format!("{}{path}", rule.hostname),
                    format!("https://{}", rule.hostname),
                );
            }
        }
        handle.set_error(None);
        tracing::info!(
            version,
            routes = next.rules.len(),
            "Cloudflare configuration applied"
        );
        *current = next;
        Ok(version)
    })
}

pub(crate) async fn run_service(
    cfg: crate::client::ClientConfig,
    handle: ClientHandle,
    cancel: CancellationToken,
    reload: &mut tokio::sync::mpsc::Receiver<crate::client::service::ReloadRequest>,
) -> Result<()> {
    run_service_with(cfg, handle, cancel, reload, run).await
}

async fn run_service_with<F, Fut>(
    mut cfg: crate::client::ClientConfig,
    handle: ClientHandle,
    cancel: CancellationToken,
    reload: &mut tokio::sync::mpsc::Receiver<crate::client::service::ReloadRequest>,
    start: F,
) -> Result<()>
where
    F: Fn(Config, ClientHandle, CancellationToken) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    loop {
        let child = cancel.child_token();
        let running = start(
            cfg.cloudflare.clone().expect("cloudflare mode"),
            handle.clone(),
            child.clone(),
        );
        tokio::pin!(running);
        let request = loop {
            let request = tokio::select! {
                result = &mut running => return result,
                _ = cancel.cancelled() => { child.cancel(); return running.await; }
                request = reload.recv() => request,
            };
            let Some(mut request) = request else {
                child.cancel();
                return running.await;
            };
            request.cfg.prepare_runtime(&request.config_path);
            let validation = request.cfg.validate().and_then(|_| {
                anyhow::ensure!(
                    request.cfg.cloudflare.is_some(),
                    "Disconnect before changing connector mode"
                );
                Ok(())
            });
            if let Err(error) = validation {
                let _ = request.reply.send(Err(error));
                continue;
            }
            break request;
        };
        child.cancel();
        if tokio::time::timeout(Duration::from_secs(3), &mut running)
            .await
            .is_err()
        {
            let _ = request.reply.send(Err(anyhow::anyhow!(
                "Cloudflare shutdown timed out during reload"
            )));
            bail!("Cloudflare shutdown timed out during reload");
        }
        cfg = request.cfg;
        handle.clear_tunnel_remotes();
        handle.set_status(ClientStatus::Reconnecting);
        let _ = request.reply.send(Ok(crate::client::ReloadOutcome {
            level: crate::client::ReloadLevel::ReconnectControl,
            connection_settings_changed: true,
            ..Default::default()
        }));
    }
}
