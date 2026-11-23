use anyhow::Result;
use astrave_desktop::client::{ClientConfig, ClientHandle, StartOptions, local_control};
use astrave_desktop::core::logging;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
mod cloud_cert;
mod peer;

#[derive(Parser, Debug)]
#[command(
    name = "astrave",
    about = "astrave client",
    after_help = "Examples:\n  \
        astrave -c conf/astrave.toml\n  \
        astrave reload -c conf/astrave.toml\n  \
        astrave verify -c conf/astrave.toml"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short, long, value_name = "FILE", global = true)]
    config: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Reload,
    Verify,
    /// Inspect a DNS provider or issue/renew a gateway certificate with lego.
    CloudCert {
        #[arg(value_enum)]
        action: cloud_cert::Action,
        /// Location of the lego executable (defaults to lego on PATH).
        #[arg(long, default_value = "lego")]
        lego: PathBuf,
    },
    /// Bridge a local TCP service over a mutually authenticated direct QUIC link.
    Peer,
    /// Introduce a configured pair of peers using their observed UDP addresses.
    Rendezvous,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Some(Command::Peer | Command::Rendezvous)) {
        let path = cli
            .config
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("peer and rendezvous require -c FILE"))?;
        return peer::run(path, matches!(cli.command, Some(Command::Rendezvous))).await;
    }
    let config_path = astrave_desktop::client::resolve_client_config_path(cli.config.as_deref())?;

    match cli.command {
        None => run(config_path).await,
        Some(Command::Reload) => reload(config_path).await,
        Some(Command::Verify) => verify(config_path),
        Some(Command::CloudCert { action, lego }) => cloud_cert::run(&config_path, action, &lego),
        Some(Command::Peer | Command::Rendezvous) => unreachable!(),
    }
}

async fn run(config_path: PathBuf) -> Result<()> {
    let cfg = ClientConfig::load(&config_path)?;
    let _guard = logging::init(&cfg.log)?;

    tracing::info!(config = %config_path.display(), "config loaded");
    tracing::info!(
        server = %cfg.server_endpoint(),
        protocol = %cfg.transport.protocol,
        tunnels = cfg.tunnels.len(),
        "starting astrave"
    );

    ClientHandle::new()
        .run_foreground(
            cfg,
            config_path,
            StartOptions {
                local_control: true,
            },
        )
        .await
}

async fn reload(config_path: PathBuf) -> Result<()> {
    let outcome = local_control::reload_via_socket(&config_path).await?;
    println!(
        "reload ok: added={} removed={} updated={} mode={}",
        outcome.added.len(),
        outcome.removed.len(),
        outcome.updated.len(),
        outcome.level.label()
    );
    if outcome.connection_settings_changed {
        println!("client settings applied via reconnect");
    }
    if !outcome.failed.is_empty() {
        for (name, err) in &outcome.failed {
            eprintln!("  failed {name}: {err}");
        }
        std::process::exit(1);
    }
    Ok(())
}

fn verify(config_path: PathBuf) -> Result<()> {
    ClientConfig::load(&config_path)?;
    println!("config ok: {}", config_path.display());
    Ok(())
}
