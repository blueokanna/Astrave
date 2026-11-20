use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, copy_bidirectional_with_sizes};

const JOIN_BUF: usize = 32 * 1024;

pub async fn join<A, B>(mut a: A, mut b: B) -> std::io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    copy_bidirectional_with_sizes(&mut a, &mut b, JOIN_BUF, JOIN_BUF).await
}

// Server accounting includes bytes read before an I/O error. Only this task
// owns the stream, so these counters need neither atomics nor heap storage.
struct Metered<S> {
    stream: S,
    read: u64,
}

impl<S: AsyncRead + Unpin> AsyncRead for Metered<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.stream).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = result {
            self.read += (buf.filled().len() - before) as u64;
        }
        result
    }
}
impl<S: AsyncWrite + Unpin> AsyncWrite for Metered<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

pub async fn join_counted<A, B>(a: A, b: B) -> (u64, u64, Option<std::io::Error>)
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let mut a = Metered { stream: a, read: 0 };
    let mut b = Metered { stream: b, read: 0 };

    let err = copy_bidirectional_with_sizes(&mut a, &mut b, JOIN_BUF, JOIN_BUF)
        .await
        .err();

    (a.read, b.read, err)
}
