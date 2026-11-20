use crate::client::handle::ClientHandle;
use crate::client::reload::ReloadOutcome;
#[cfg(unix)]
use anyhow::bail;
use anyhow::{Context, Result, anyhow};
use std::io;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

const ENV_CONTROL_SOCKET: &str = "ASTRAVE_CONTROL_SOCKET";
#[cfg(not(windows))]
const ENV_STATE_DIR: &str = "ASTRAVE_STATE_DIR";
#[cfg(not(windows))]
const SOCKET_FILE: &str = "control.sock";
#[cfg(windows)]
const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\astrave-control";

#[derive(Debug, Default, nextjson::NsonDeserialize, nextjson::NsonSerialize)]
struct ControlRequest {
    #[njson(default)]
    op: String,
    #[njson(default)]
    config: Option<String>,
}

#[derive(Debug, Default, nextjson::NsonDeserialize, nextjson::NsonSerialize)]
struct ControlResponse {
    ok: bool,
    #[njson(default, skip_serializing_if = "Option::is_none")]
    outcome: Option<ReloadOutcome>,
    #[njson(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub fn default_socket_path() -> PathBuf {
    if let Some(path) = env_path(ENV_CONTROL_SOCKET) {
        return path;
    }

    #[cfg(windows)]
    {
        PathBuf::from(DEFAULT_PIPE_NAME)
    }

    #[cfg(unix)]
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let dir = dir.to_string_lossy();
        if !dir.trim().is_empty() {
            return PathBuf::from(dir.as_ref())
                .join("astrave")
                .join(SOCKET_FILE);
        }
    }

    #[cfg(not(windows))]
    {
        return state_dir().join("run").join(SOCKET_FILE);
    }
}

#[cfg(not(windows))]
fn state_dir() -> PathBuf {
    if let Some(dir) = env_path(ENV_STATE_DIR) {
        return dir;
    }

    if let Some(home) = user_home() {
        #[cfg(target_os = "macos")]
        {
            return home
                .join("Library")
                .join("Application Support")
                .join("com.astrave.client");
        }
        #[cfg(not(target_os = "macos"))]
        {
            return home.join(".config").join("astrave");
        }
    }

    std::env::temp_dir().join("astrave")
}

#[cfg(not(windows))]
fn user_home() -> Option<PathBuf> {
    for key in ["HOME", "USERPROFILE"] {
        if let Some(value) = std::env::var_os(key) {
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

#[cfg(unix)]
fn is_stale_socket_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::NotFound
            | io::ErrorKind::AddrNotAvailable
    )
}

pub async fn serve(
    socket_path: PathBuf,
    handle: ClientHandle,
    cancel: CancellationToken,
) -> Result<()> {
    #[cfg(unix)]
    {
        serve_unix(socket_path, handle, cancel).await
    }
    #[cfg(windows)]
    {
        serve_windows(socket_path, handle, cancel).await
    }
}

#[cfg(unix)]
async fn serve_unix(
    socket_path: PathBuf,
    handle: ClientHandle,
    cancel: CancellationToken,
) -> Result<()> {
    use tokio::net::{UnixListener, UnixStream};

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create socket dir {}", parent.display()))?;
    }

    if socket_path.exists() {
        match UnixStream::connect(&socket_path).await {
            Ok(stream) => {
                drop(stream);
                bail!(
                    "control socket {} is already in use; another astrave client may be running",
                    socket_path.display()
                );
            }
            Err(e) if is_stale_socket_error(&e) => {
                std::fs::remove_file(&socket_path).with_context(|| {
                    format!("remove stale control socket {}", socket_path.display())
                })?;
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("probe control socket {}", socket_path.display()));
            }
        }
    }

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("bind control socket {}", socket_path.display()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    }

    tracing::info!(path = %socket_path.display(), "control socket listening");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let hc = handle.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, hc).await {
                        tracing::debug!(error = %e, "control connection ended");
                    }
                });
            }
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

#[cfg(windows)]
async fn serve_windows(
    socket_path: PathBuf,
    handle: ClientHandle,
    cancel: CancellationToken,
) -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let name = socket_path.to_string_lossy().into_owned();
    tracing::info!(path = %name, "control named pipe listening");

    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&name)
        .with_context(|| format!("create control named pipe {name}"))?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            connected = server.connect() => {
                connected.with_context(|| format!("accept control named pipe {name}"))?;
                let connected_server = server;
                server = ServerOptions::new()
                    .create(&name)
                    .with_context(|| format!("recreate control named pipe {name}"))?;
                let hc = handle.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(connected_server, hc).await {
                        tracing::debug!(error = %e, "control connection ended");
                    }
                });
            }
        }
    }

    Ok(())
}

async fn read_control_line(reader: impl AsyncRead + Unpin, limit: usize) -> Result<String> {
    let mut line = String::new();
    BufReader::new(reader.take(limit as u64 + 1))
        .read_line(&mut line)
        .await?;
    anyhow::ensure!(line.len() <= limit, "control message exceeds {limit} bytes");
    anyhow::ensure!(!line.trim().is_empty(), "empty control message");
    Ok(line)
}

async fn handle_connection<S>(stream: S, handle: ClientHandle) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let line = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read_control_line(reader, 65536),
    )
    .await??;

    let req: ControlRequest =
        nextjson::nextdecode(line.as_bytes()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let resp = match req.op.as_str() {
        "reload" => handle_reload(&handle, req.config.as_deref()).await,
        "ping" => ControlResponse {
            ok: true,
            outcome: None,
            error: None,
        },
        other => ControlResponse {
            ok: false,
            outcome: None,
            error: Some(format!("unknown op: {other}")),
        },
    };

    let body = nextjson::nextencode(&resp).map_err(|e| anyhow::anyhow!("{e}"))?;
    writer.write_all(&body).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}

async fn handle_reload(handle: &ClientHandle, config: Option<&str>) -> ControlResponse {
    let path = match config {
        Some(p) => PathBuf::from(p),
        None => {
            return ControlResponse {
                ok: false,
                outcome: None,
                error: Some("config path is required".into()),
            };
        }
    };
    match handle.reload_from_path(&path).await {
        Ok(outcome) => ControlResponse {
            ok: outcome.succeeded(),
            outcome: Some(outcome),
            error: None,
        },
        Err(e) => ControlResponse {
            ok: false,
            outcome: None,
            error: Some(e.to_string()),
        },
    }
}

pub async fn reload_via_socket(config_path: &Path) -> Result<ReloadOutcome> {
    let socket_path = default_socket_path();
    let stream = connect_control(&socket_path).await.with_context(|| {
        format!(
            "connect to control socket at {} (start astrave with -c first)",
            socket_path.display()
        )
    })?;
    let (reader, mut writer) = tokio::io::split(stream);
    let req = ControlRequest {
        op: "reload".into(),
        config: Some(config_path.to_string_lossy().into_owned()),
    };
    writer
        .write_all(&nextjson::nextencode(&req).map_err(|e| anyhow::anyhow!("{e}"))?)
        .await?;
    writer.write_all(b"\n").await?;

    let line = tokio::time::timeout(
        std::time::Duration::from_secs(125),
        read_control_line(reader, 4 * 1024 * 1024),
    )
    .await??;
    let resp: ControlResponse =
        nextjson::nextdecode(line.as_bytes()).map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(outcome) = resp.outcome {
        if resp.ok {
            Ok(outcome)
        } else {
            Err(anyhow!(
                "reload completed with errors: {:?}",
                outcome.failed
            ))
        }
    } else {
        Err(anyhow!(
            resp.error.unwrap_or_else(|| "reload failed".into())
        ))
    }
}

#[cfg(unix)]
async fn connect_control(socket_path: &Path) -> Result<tokio::net::UnixStream> {
    Ok(tokio::net::UnixStream::connect(socket_path).await?)
}

#[cfg(windows)]
async fn connect_control(
    socket_path: &Path,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    use std::time::Duration;
    use tokio::net::windows::named_pipe::ClientOptions;

    let name = socket_path.to_string_lossy();
    let mut last_err = None;
    for _ in 0..20 {
        match ClientOptions::new().open(name.as_ref()) {
            Ok(client) => return Ok(client),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock || is_pipe_busy(&e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(last_err
        .map(Into::into)
        .unwrap_or_else(|| anyhow!("named pipe busy: {name}")))
}

#[cfg(windows)]
fn is_pipe_busy(err: &io::Error) -> bool {
    err.raw_os_error() == Some(231)
}
