use anyhow::{Context, Result, ensure};
use std::{ffi::OsString, path::Path, process::Command};

#[derive(Debug, clap::Subcommand)]
pub enum Provider {
    /// Start one named endpoint from an ngrok agent configuration (-c FILE).
    Ngrok { endpoint: String },
    /// Start NATAPP with its native config.ini (-c FILE).
    Natapp,
    /// Start the Sunny agent using its device authorization token from SUNNY_TOKEN.
    Sunny,
}

impl Provider {
    pub fn run(&self, config: Option<&Path>, executable: Option<&Path>) -> Result<()> {
        let name = match self {
            Self::Ngrok { .. } => "ngrok",
            Self::Natapp => "natapp",
            Self::Sunny => "sunny",
        };
        let default_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        let program = executable.unwrap_or_else(|| Path::new(&default_name));
        #[cfg(windows)]
        ensure!(
            program
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe")),
            "--agent must point to a native .exe"
        );
        let mut command = Command::new(program);
        match self {
            Self::Ngrok { endpoint } => {
                ensure!(
                    !endpoint.is_empty() && !endpoint.starts_with('-'),
                    "expected a named ngrok endpoint"
                );
                command
                    .args(["start", "--config"])
                    .arg(config_file(config)?)
                    .arg("--")
                    .arg(endpoint);
            }
            Self::Natapp => {
                let mut arg = OsString::from("-config=");
                arg.push(config_file(config)?);
                command.arg(arg);
            }
            Self::Sunny => {
                ensure!(
                    config.is_none(),
                    "Sunny uses SUNNY_TOKEN and dashboard routes"
                );
                let token = std::env::var("SUNNY_TOKEN")
                    .context("set SUNNY_TOKEN to the Sunny device token")?;
                ensure!(
                    !token.is_empty()
                        && token.len() <= 4096
                        && token.bytes().all(|b| b.is_ascii_graphic()),
                    "invalid SUNNY_TOKEN"
                );
                command.env_remove("SUNNY_TOKEN").arg("-t").arg(token);
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            Err(command.exec()).with_context(|| format!("start {name} agent"))
        }
        #[cfg(not(unix))]
        {
            let status = command
                .status()
                .with_context(|| format!("start {name} agent"))?;
            ensure!(status.success(), "{name} agent exited with {status}");
            Ok(())
        }
    }
}

fn config_file(path: Option<&Path>) -> Result<&Path> {
    let path = path.context("supply the provider configuration with -c FILE")?;
    ensure!(
        path.is_file(),
        "provider configuration file not found: {}",
        path.display()
    );
    Ok(path)
}
