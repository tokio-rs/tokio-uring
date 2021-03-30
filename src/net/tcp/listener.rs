use crate::driver::Op;
use crate::net::TcpStream;

use std::io;
use std::net::SocketAddr;
use std::os::unix::io::{RawFd, FromRawFd, IntoRawFd};

pub struct TcpListener {
    /// Decorates a `std` TcpListener and uring-ifies it.
    fd: RawFd,
}

impl TcpListener {
    pub fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
        let std = std::net::TcpListener::bind(addr)?;

        Ok(TcpListener {
            fd: std.into_raw_fd(),
        })
    }

    pub async fn accept(&self) -> io::Result<TcpStream> {
        let op = Op::accept(self.fd)?;
        let completion = op.await;
        let fd = completion.result?;

        Ok(unsafe { TcpStream::from_raw_fd(fd as _) })
    }

    /// Close the file
    pub async fn close(mut self) -> io::Result<()> {
        let op = Op::close(self.fd)?;
        let completion = op.await;

        self.fd = 0;
        drop(self);

        completion.result.map(|_| ())
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        if self.fd == 0 { return }

        // TODO: warn? Do something better?
        let _ = unsafe { std::fs::File::from_raw_fd(self.fd) };
    }
}