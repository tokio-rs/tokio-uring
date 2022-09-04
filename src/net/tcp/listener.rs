use super::TcpStream;
use crate::driver::Socket;
use std::{io, net::SocketAddr};

/// A TCP socket server, listening for connections.
///
/// You can accept a new connection by using the [`accept`](`TcpListener::accept`)
/// method.
///
/// # Examples
///
/// ```
/// use tokio_uring::net::TcpListener;
/// use tokio_uring::net::TcpStream;
///
/// let listener = TcpListener::bind("127.0.0.1:2345".parse().unwrap()).unwrap();
///
/// tokio_uring::start(async move {
///     let tx_fut = TcpStream::connect("127.0.0.1:2345".parse().unwrap());
///
///     let rx_fut = listener.accept();
///
///     let (tx, (rx, _)) = tokio::try_join!(tx_fut, rx_fut).unwrap();
///
///     tx.write(b"test" as &'static [u8]).await.0.unwrap();
///
///     let (_, buf) = rx.read(vec![0; 4]).await;
///
///     assert_eq!(buf, b"test");
/// });
/// ```
pub struct TcpListener {
    inner: Socket,
}

impl TcpListener {
    /// Creates a new TcpListener, which will be bound to the specified address.
    ///
    /// The returned listener is ready for accepting connections.
    ///
    /// Binding with a port number of 0 will request that the OS assigns a port
    /// to this listener.
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        let socket = Socket::bind(addr, libc::SOCK_STREAM)?;
        socket.listen(1024)?;
        Ok(TcpListener { inner: socket })
    }

    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to figure out
    /// which port was actually bound. The implementation relies on the standard net
    /// system call so is blocking.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use tokio_uring::net::TcpListener;
    ///
    /// let listener = TcpListener::bind("127.0.0.1:8080".parse().unwrap()).unwrap();
    ///
    /// let addr = listener.local_addr().expect("Couldn't get local address");
    /// assert_eq!(addr, SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));
    /// ```
    #[cfg(unix)]
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        use std::os::unix::io::{AsRawFd, FromRawFd};

        let fd = self.inner.as_raw_fd();
        // SAFETY: Our fd is the handle the kernel has given us for a TcpListener.
        // Create a std::net::TcpListener long enough to call its local_addr method
        // and then forget it so the socket is not closed here.
        let l = unsafe { std::net::TcpListener::from_raw_fd(fd) };
        let local_addr = l.local_addr();
        std::mem::forget(l);
        local_addr
    }

    /// Accepts a new incoming connection from this listener.
    ///
    /// This function will yield once a new TCP connection is established. When
    /// established, the corresponding [`TcpStream`] and the remote peer's
    /// address will be returned.
    ///
    /// [`TcpStream`]: struct@crate::net::TcpStream
    pub async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let (socket, socket_addr) = self.inner.accept().await?;
        let stream = TcpStream { inner: socket };
        let socket_addr = socket_addr.ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Could not get socket IP address")
        })?;
        Ok((stream, socket_addr))
    }
}
