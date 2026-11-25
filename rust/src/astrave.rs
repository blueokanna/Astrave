use anyhow::Result;
use astrave_desktop::client::{ClientConfig, ClientHandle, StartOptions, local_control};
use astrave_desktop::core::logging;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
mod cloud_cert;
#[cfg(feature = "p2p")]
mod p2p;
mod peer;
mod provider;

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
    /// Forward a loopback TCP service between two pinned libp2p identities.
    #[cfg(feature = "p2p")]
    P2p,
    /// Create an Ed25519 identity and print its public Peer ID.
    #[cfg(feature = "p2p")]
    P2pKeygen {
        file: PathBuf,
    },
    /// Run an installed tunnel provider's agent in the foreground.
    Provider {
        #[command(subcommand)]
        provider: provider::Provider,
        /// Path to the provider's native executable (default: look it up on PATH).
        #[arg(long, global = true)]
        agent: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let profile = || astrave_desktop::client::resolve_client_config_path(cli.config.as_deref());
    let explicit = || {
        cli.config
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("supply the endpoint configuration with -c FILE"))
    };
    match cli.command {
        None => block_on(run(profile()?)),
        Some(Command::Reload) => block_on(reload(profile()?)),
        Some(Command::Verify) => verify(profile()?),
        Some(Command::CloudCert { action, lego }) => cloud_cert::run(&profile()?, action, &lego),
        Some(Command::Peer) => block_on(peer::run(explicit()?, false)),
        Some(Command::Rendezvous) => block_on(peer::run(explicit()?, true)),
        #[cfg(feature = "p2p")]
        Some(Command::P2pKeygen { file }) => {
            println!("{}", p2p::keygen(&file)?);
            Ok(())
        }
        #[cfg(feature = "p2p")]
        Some(Command::P2p) => block_on(p2p::run(Path::new(explicit()?))),
        Some(Command::Provider { provider, agent }) => {
            provider.run(cli.config.as_deref().map(Path::new), agent.as_deref())
        }
    }
}

fn block_on(task: impl Future<Output = Result<()>>) -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(task)
}

async fn run(config_path: PathBuf) -> Result<()> {
    let cfg = ClientConfig::load(&config_path)?;
    let _guard = logging::init(&cfg.log)?;

    tracing::info!(config = %config_path.display(), "config loaded");
    tracing::info!(
        server = %if cfg.cloudflare.is_some() { "Cloudflare Tunnel".to_string() } else { cfg.server_endpoint() },
        protocol = %cfg.cloudflare.as_ref().map_or(cfg.transport.protocol.as_str(), |c| c.protocol.as_str()),
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
