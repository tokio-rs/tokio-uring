use super::TcpStream;
use crate::io::{SharedFd, Socket};
use std::{
    io,
    net::SocketAddr,
    os::unix::prelude::{AsRawFd, FromRawFd, RawFd},
};

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
///     let (tx_ch, rx_ch) = tokio::sync::oneshot::channel();
///
///     tokio_uring::spawn(async move {
///         let (rx, _) = listener.accept().await.unwrap();
///         if let Err(_) = tx_ch.send(rx) {
///             panic!("The receiver dropped");
///         }
///     });
///     tokio::task::yield_now().await; // Ensure the listener.accept().await has been kicked off.
///
///     let tx = TcpStream::connect("127.0.0.1:2345".parse().unwrap()).await.unwrap();
///     let rx = rx_ch.await.expect("The spawned task expected to send a TcpStream");
///
///     tx.write(b"test" as &'static [u8]).submit().await.0.unwrap();
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

    /// Creates new `TcpListener` from a previously bound `std::net::TcpListener`.
    ///
    /// This function is intended to be used to wrap a TCP listener from the
    /// standard library in the tokio-uring equivalent. The conversion assumes nothing
    /// about the underlying socket; it is left up to the user to decide what socket
    /// options are appropriate for their use case.
    ///
    /// This can be used in conjunction with socket2's `Socket` interface to
    /// configure a socket before it's handed off, such as setting options like
    /// `reuse_address` or binding to multiple addresses.
    ///
    /// # Example
    ///
    /// ```
    /// tokio_uring::start(async {
    ///     let address: std::net::SocketAddr = "[::0]:8443".parse().unwrap();
    ///     let socket = tokio::net::TcpSocket::new_v6().unwrap();
    ///     socket.set_reuseaddr(true).unwrap();
    ///     socket.set_reuseport(true).unwrap();
    ///     socket.bind(address).unwrap();
    ///
    ///     let listener = socket.listen(1024).unwrap();
    ///
    ///     let listener = tokio_uring::net::TcpListener::from_std(listener.into_std().unwrap());
    /// })
    /// ```
    pub fn from_std(socket: std::net::TcpListener) -> Self {
        let inner = Socket::from_std(socket);
        Self { inner }
    }

    pub(crate) fn from_socket(inner: Socket) -> Self {
        Self { inner }
    }

    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to
    /// figure out which port was actually bound.
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
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
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

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        TcpListener::from_socket(Socket::from_shared_fd(SharedFd::new(fd)))
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
