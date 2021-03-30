use crate::driver;

use tokio::io::{AsyncRead, AsyncBufRead, AsyncWrite, ReadBuf};
use std::io;
use std::os::unix::io::{RawFd, FromRawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct TcpStream {
    stream: driver::Stream,
}

impl TcpStream {
    pub async fn write(&self, buf: crate::buf::Slice) -> crate::BufResult<usize> {
        let op = driver::Op::write_at(self.stream.as_raw_fd(), buf, 0).unwrap();
        op.write().await
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>
    ) -> Poll<io::Result<()>> {
        self.stream.poll_read(cx, buf)
    }
}

impl AsyncBufRead for TcpStream {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        self.get_mut().stream.poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        self.stream.consume(amt)
    }    
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8]
    ) -> Poll<io::Result<usize>> {
        self.stream.poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>
    ) -> Poll<io::Result<()>> {
        self.stream.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        unimplemented!();
    }
}

impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpStream {
        let stream = driver::Stream::new(fd);
        TcpStream { stream }
    }
}