#[path = "server_dashboard_model.rs"]
pub(crate) mod model;
#[path = "server_dashboard_routes.rs"]
mod routes;
#[path = "server_dashboard_snapshot.rs"]
mod snapshot;
#[path = "server_dashboard_views.rs"]
mod views;

use crate::service::Service;
use anyhow::{Context, Result};
use astrave_desktop::core::config::DashboardConfig;
use std::sync::Arc;

pub async fn run(svc: Arc<Service>, cfg: DashboardConfig) -> Result<()> {
    let addr = format!("{}:{}", cfg.addr, cfg.port);
    let state = Arc::new(DashState { svc, cfg });
    let runtime = tokio::runtime::Handle::current();

    let server = courierust::courierust_server::Server::bind(addr.as_str())
        .with_context(|| format!("bind dashboard on {addr}"))?;
    tracing::info!(%addr, user = %state.cfg.user, "dashboard listening");

    let handle = server
        .serve_background(routes::Dashboard::new(state, runtime))
        .context("start dashboard server")?;

    // The dashboard lives as long as the process; joining keeps the error path
    // observable instead of silently losing the server thread.
    tokio::task::spawn_blocking(move || handle.join())
        .await
        .context("dashboard server thread panicked")?
        .context("dashboard server stopped")?;
    Ok(())
}

#[derive(Clone)]
pub struct DashState {
    pub svc: Arc<Service>,
    pub cfg: DashboardConfig,
}
