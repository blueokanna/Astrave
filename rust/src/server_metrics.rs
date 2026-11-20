#[path = "server_metrics_counter.rs"]
mod counter;
#[path = "server_metrics_history.rs"]
mod history;
#[path = "server_metrics_mem.rs"]
mod mem;

pub use mem::{MemMetrics, TrafficWindow, TunnelTrafficHistory};
pub const RESERVE_DAYS: usize = 7;
pub const RESERVE_HOURS: usize = 24;

pub async fn join_and_record<A, B>(
    metrics: &std::sync::Arc<MemMetrics>,
    name: &str,
    tunnel_type: &str,
    ingress: A,
    data: B,
) -> std::io::Result<(u64, u64)>
where
    A: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    B: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let _guard = metrics.track_connection(name, tunnel_type);
    let (to_data, from_data, err) = astrave_desktop::core::io::join_counted(ingress, data).await;
    metrics.record_traffic(name, to_data, from_data);
    match err {
        None => Ok((to_data, from_data)),
        Some(e) if is_benign_close(&e) => Ok((to_data, from_data)),
        Some(e) => Err(e),
    }
}

fn is_benign_close(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::UnexpectedEof
    )
}
