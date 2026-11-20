use super::Control;
use anyhow::{Result, anyhow};
use astrave_core::msg::{self, Codec, Message, ReqDataConn, StartDataConn};
use astrave_core::transport::DynStream;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

/// A pooled data connection plus the codec its handshake used.
pub struct DataConn {
    pub stream: DynStream,
    pub codec: Codec,
}

impl Control {
    pub async fn push_data_conn(&self, stream: DynStream, codec: Codec) {
        if self.data_tx.try_send(DataConn { stream, codec }).is_ok() {
            self.data_notify.notify_one();
        }
    }

    async fn try_pop_data(&self) -> Option<DataConn> {
        let mut rx = self.data_rx.lock().await;
        rx.try_recv().ok()
    }

    async fn spawn_refill(self: &Arc<Self>) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let ctl = Arc::clone(self);
        self.spawn_bg(async move {
            if ctl.closed.load(Ordering::SeqCst) {
                return;
            }
            let _ = ctl.request_data_conn().await;
        })
        .await;
    }

    pub async fn get_data_conn(self: &Arc<Self>) -> Result<DataConn> {
        if let Some(conn) = self.try_pop_data().await {
            self.spawn_refill().await;
            return Ok(conn);
        }

        self.request_data_conn().await?;

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let notified = self.data_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.closed.load(Ordering::SeqCst) {
                return Err(anyhow!("control closed while waiting for data connection"));
            }
            if let Some(conn) = self.try_pop_data().await {
                self.spawn_refill().await;
                return Ok(conn);
            }
            timeout_at(deadline, notified)
                .await
                .map_err(|_| anyhow!("timeout waiting for data connection"))?;
        }
    }

    pub(super) async fn request_data_conn(&self) -> Result<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(anyhow!("control closed"));
        }
        let mut writer = self.writer.lock().await;
        msg::write_msg(
            &mut *writer,
            &Message::ReqDataConn(ReqDataConn {}),
            self.codec(),
        )
        .await?;
        Ok(())
    }

    pub async fn start_data_conn(
        &self,
        conn: DataConn,
        tunnel_name: &str,
        src_addr: String,
        src_port: u16,
        dst_addr: String,
        dst_port: u16,
    ) -> Result<DataConn> {
        let DataConn { mut stream, codec } = conn;
        msg::write_msg(
            &mut stream,
            &Message::StartDataConn(StartDataConn {
                tunnel_name: tunnel_name.to_string(),
                src_addr,
                src_port,
                dst_addr,
                dst_port,
                error: String::new(),
            }),
            codec,
        )
        .await?;
        Ok(DataConn { stream, codec })
    }
}
