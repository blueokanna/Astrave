#[path = "server_access.rs"]
mod access;
#[path = "server_control.rs"]
mod control;
#[path = "server_dashboard.rs"]
mod dashboard;
#[path = "server_metrics.rs"]
mod metrics;
#[path = "server_service.rs"]
mod service;
#[path = "server_tunnel.rs"]
mod tunnel;

use anyhow::Result;
use astrave_desktop::core::config::ServerConfig;
use astrave_desktop::core::logging;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "astrave-server",
    about = "astrave server",
    after_help = "Example:\n  astrave-server -c conf/astrave-server.toml"
)]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let (cfg, config_src) = load_server_config(&args)?;
    let _guard = logging::init(&cfg.log)?;

    match config_src {
        Some(path) => tracing::info!(config = %path, "config loaded"),
        None => tracing::info!("using built-in defaults"),
    }
    tracing::info!(
        listen = %cfg.listen,
        quic_port = cfg.quic_port,
        kcp_port = cfg.kcp_port,
        http_gw = cfg.http_gw_port,
        https_gw = cfg.https_gw_port,
        "starting astrave-server"
    );

    service::Service::new(cfg)?.run().await
}

fn load_server_config(args: &Args) -> Result<(ServerConfig, Option<String>)> {
    if let Some(path) = args
        .config
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        return Ok((ServerConfig::load(path)?, Some(path.to_string())));
    }

    let mut cfg = ServerConfig::default();
    cfg.complete();
    cfg.validate()?;
    Ok((cfg, None))
}
