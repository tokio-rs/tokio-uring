use std::io::Result as IoResult;
use std::mem::ManuallyDrop;
use std::os::unix::io::AsRawFd;

use tokio::net::ToSocketAddrs;

use crate::buf::{IoBuf, IoBufMut};
use crate::driver::{Op, SharedFd};

/// An `io_uring`-backed implementation of the User Datagram Protocol
pub struct UdpSocket {
    fd: SharedFd,
    _inner_sock: ManuallyDrop<std::net::UdpSocket>,
}

impl UdpSocket {
    /// Bind a socket to specified address.
    pub async fn bind<A>(addr: A) -> IoResult<Self>
    where
        A: ToSocketAddrs,
    {
        // fall back on tokio for this, it just does a dns lookup via thread pools
        let sock = tokio::net::UdpSocket::bind(addr).await?;

        let raw_fd = sock.as_raw_fd();

        let fd = SharedFd::new(raw_fd);

        let std = sock.into_std().unwrap();

        Ok(Self {
            fd,
            _inner_sock: ManuallyDrop::new(std),
        })
    }

    /// Connect a bound socket to a remote endpoint.
    pub async fn connect<A>(&self, addr: A) -> IoResult<()>
    where
        A: ToSocketAddrs,
    {
        // use tokio lookup for addr
        let addr = tokio::net::lookup_host(addr)
            .await?
            .next()
            .map(Ok)
            .unwrap_or(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Unable to resolve any addresses from input!",
            )))?;

        let op = Op::connect(self.fd.clone(), &addr)?;

        op.await.result.map(|_| ())
    }

    /// Send a packet to a remote endpoint.
    ///
    /// Returns an error if the socket is not connected.
    pub async fn send<B>(&self, buf: B) -> crate::BufResult<usize, B>
    where
        B: IoBuf,
    {
        let op = Op::send(&self.fd, buf).unwrap();
        op.complete_send().await
    }

    /// Receive a packet from a remote endpoint.
    pub async fn recv<B>(&self, buf: B) -> crate::BufResult<usize, B>
    where
        B: IoBufMut,
    {
        let op = Op::recv(&self.fd, buf).unwrap();

        op.complete_read().await
    }
}
