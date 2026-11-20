use anyhow::Result;
use astrave_client::{ClientConfig, ClientHandle, StartOptions, local_control};
use astrave_core::logging;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = astrave_client::resolve_client_config_path(cli.config.as_deref())?;

    match cli.command {
        None => run(config_path).await,
        Some(Command::Reload) => reload(config_path).await,
        Some(Command::Verify) => verify(config_path),
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
