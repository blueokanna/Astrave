#[path = "logging_rotate_file.rs"]
mod rotate_file;

use crate::core::config::LogConfig;
use anyhow::{Context, anyhow};
use rotate_file::RotateFileWriter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[must_use = "keep LogGuard alive until shutdown so log files flush"]
pub struct LogGuard {
    _worker: Option<WorkerGuard>,
}

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(
            w,
            "{}",
            jiff::ZonedDateTime::now().strftime("%Y-%m-%dT%H:%M:%S%.3f%:z")
        )
    }
}

pub fn init(cfg: &LogConfig) -> anyhow::Result<LogGuard> {
    let filter = env_filter(&cfg.level)?;

    if !cfg.file_enabled() {
        fmt().with_timer(LocalTimer).with_env_filter(filter).init();
        return Ok(LogGuard { _worker: None });
    }

    let path = cfg.file.trim();
    let rotate = RotateFileWriter::open(path, cfg.max_days)
        .with_context(|| format!("failed to open log file '{path}'"))?;

    let (non_blocking, worker) = tracing_appender::non_blocking(rotate);

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_timer(LocalTimer)
                .with_ansi(false)
                .with_writer(std::io::stdout.and(non_blocking)),
        )
        .init();

    Ok(LogGuard {
        _worker: Some(worker),
    })
}

fn env_filter(level: &str) -> anyhow::Result<EnvFilter> {
    match EnvFilter::try_from_default_env() {
        Ok(filter) => Ok(filter),
        Err(_) => EnvFilter::try_new(level.trim())
            .map_err(|e| anyhow!("invalid log.level {:?}: {e}", level)),
    }
}
