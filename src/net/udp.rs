use crate::{
    buf::fixed::FixedBuf,
    buf::{BoundedBuf, BoundedBufMut, IoBuf},
    io::{SharedFd, Socket},
};
use socket2::SockAddr;
use std::{
    io,
    net::SocketAddr,
    os::unix::prelude::{AsRawFd, FromRawFd, RawFd},
};

/// A UDP socket.
///
/// UDP is "connectionless", unlike TCP. Meaning, regardless of what address you've bound to, a `UdpSocket`
/// is free to communicate with many different remotes. In tokio there are basically two main ways to use `UdpSocket`:
///
/// * one to many: [`bind`](`UdpSocket::bind`) and use [`send_to`](`UdpSocket::send_to`)
///   and [`recv_from`](`UdpSocket::recv_from`) to communicate with many different addresses
/// * one to one: [`connect`](`UdpSocket::connect`) and associate with a single address, using [`write`](`UdpSocket::write`)
///   and [`read`](`UdpSocket::read`) to communicate only with that remote address
///
/// # Examples
/// Bind and connect a pair of sockets and send a packet:
///
/// ```
/// use tokio_uring::net::UdpSocket;
/// use std::net::SocketAddr;
/// fn main() -> std::io::Result<()> {
///     tokio_uring::start(async {
///         let first_addr: SocketAddr = "127.0.0.1:2401".parse().unwrap();
///         let second_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
///
///         // bind sockets
///         let socket = UdpSocket::bind(first_addr.clone()).await?;
///         let other_socket = UdpSocket::bind(second_addr.clone()).await?;
///
///         // connect sockets
///         socket.connect(second_addr).await.unwrap();
///         other_socket.connect(first_addr).await.unwrap();
///
///         let buf = vec![0; 32];
///
///         // write data
///         let (result, _) = socket.write(b"hello world".as_slice()).await;
///         result.unwrap();
///
///         // read data
///         let (result, buf) = other_socket.read(buf).await;
///         let n_bytes = result.unwrap();
///
///         assert_eq!(b"hello world", &buf[..n_bytes]);
///
///         Ok(())
///     })
/// }
/// ```
/// Send and receive packets without connecting:
///
/// ```
/// use tokio_uring::net::UdpSocket;
/// use std::net::SocketAddr;
/// fn main() -> std::io::Result<()> {
///     tokio_uring::start(async {
///         let first_addr: SocketAddr = "127.0.0.1:2401".parse().unwrap();
///         let second_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
///
///         // bind sockets
///         let socket = UdpSocket::bind(first_addr.clone()).await?;
///         let other_socket = UdpSocket::bind(second_addr.clone()).await?;
///
///         let buf = vec![0; 32];
///
///         // write data
///         let (result, _) = socket.send_to(b"hello world".as_slice(), second_addr).await;
///         result.unwrap();
///
///         // read data
///         let (result, buf) = other_socket.recv_from(buf).await;
///         let (n_bytes, addr) = result.unwrap();
///
///         assert_eq!(addr, first_addr);
///         assert_eq!(b"hello world", &buf[..n_bytes]);
///
///         Ok(())
///     })
/// }
/// ```
pub struct UdpSocket {
    pub(super) inner: Socket,
}

impl UdpSocket {
    /// Creates a new UDP socket and attempt to bind it to the addr provided.
    pub async fn bind(socket_addr: SocketAddr) -> io::Result<UdpSocket> {
        let socket = Socket::bind(socket_addr, libc::SOCK_DGRAM)?;
        Ok(UdpSocket { inner: socket })
    }

    /// Returns the local address that this UDP socket is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to
    /// figure out which port was actually bound.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use tokio_uring::net::UdpSocket;
    ///
    /// tokio_uring::start(async {
    ///     let socket = UdpSocket::bind("127.0.0.1:8080".parse().unwrap()).await.unwrap();
    ///     let addr = socket.local_addr().expect("Couldn't get local address");
    ///     assert_eq!(addr, SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));
    /// });
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        let fd = self.inner.as_raw_fd();
        // SAFETY: Our fd is the handle the kernel has given us for a UdpSocket.
        // Create a std::net::UdpSocket long enough to call its local_addr method
        // and then forget it so the socket is not closed here.
        let s = unsafe { std::net::UdpSocket::from_raw_fd(fd) };
        let local_addr = s.local_addr();
        std::mem::forget(s);
        local_addr
    }

    /// Creates new `UdpSocket` from a previously bound `std::net::UdpSocket`.
    ///
    /// This function is intended to be used to wrap a UDP socket from the
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
    /// use socket2::{Protocol, Socket, Type};
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// use tokio_uring::net::UdpSocket;
    ///
    /// fn main() -> std::io::Result<()> {
    ///     tokio_uring::start(async {
    ///         let std_addr: SocketAddr = "127.0.0.1:2401".parse().unwrap();
    ///         let second_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    ///         let sock = Socket::new(socket2::Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    ///         sock.set_reuse_port(true)?;
    ///         sock.set_nonblocking(true)?;
    ///         sock.bind(&std_addr.into())?;
    ///
    ///         let std_socket = UdpSocket::from_std(sock.into());
    ///         let other_socket = UdpSocket::bind(second_addr).await?;
    ///
    ///         let buf = vec![0; 32];
    ///
    ///         // write data
    ///         let (result, _) = std_socket
    ///             .send_to(b"hello world".as_slice(), second_addr)
    ///             .await;
    ///         result.unwrap();
    ///
    ///         // read data
    ///         let (result, buf) = other_socket.recv_from(buf).await;
    ///         let (n_bytes, addr) = result.unwrap();
    ///
    ///         assert_eq!(addr, std_addr);
    ///         assert_eq!(b"hello world", &buf[..n_bytes]);
    ///
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub fn from_std(socket: std::net::UdpSocket) -> Self {
        let inner = Socket::from_std(socket);
        Self { inner }
    }

    pub(crate) fn from_socket(inner: Socket) -> Self {
        Self { inner }
    }

    /// Connects this UDP socket to a remote address, allowing the `write` and
    /// `read` syscalls to be used to send data and also applies filters to only
    /// receive data from the specified address.
    ///
    /// Note that usually, a successful `connect` call does not specify
    /// that there is a remote server listening on the port, rather, such an
    /// error would only be detected after the first send.
    pub async fn connect(&self, socket_addr: SocketAddr) -> io::Result<()> {
        self.inner.connect(SockAddr::from(socket_addr)).await
    }

    /// Sends data on the socket to the given address. On success, returns the
    /// number of bytes written.
    pub async fn send_to<T: BoundedBuf>(
        &self,
        buf: T,
        socket_addr: SocketAddr,
    ) -> crate::BufResult<usize, T> {
        self.inner.send_to(buf, socket_addr).await
    }

    /// Sends data on the socket. Will attempt to do so without intermediate copies.
    /// On success, returns the number of bytes written.
    ///
    /// See the linux [kernel docs](https://www.kernel.org/doc/html/latest/networking/msg_zerocopy.html)
    /// for a discussion on when this might be appropriate. In particular:
    ///
    /// > Copy avoidance is not a free lunch. As implemented, with page pinning,
    /// > it replaces per byte copy cost with page accounting and completion
    /// > notification overhead. As a result, zero copy is generally only effective
    /// > at writes over around 10 KB.
    ///
    /// Note: Using fixed buffers [#54](https://github.com/tokio-rs/tokio-uring/pull/54), avoids the page-pinning overhead
    pub async fn send_zc<T: BoundedBuf>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.send_zc(buf).await
    }

    /// Sends a message on the socket using a msghdr.
    pub async fn sendmsg_zc<T: IoBuf>(
        &self,
        io_slices: Vec<T>,
        socket_addr: SocketAddr,
        msg_control: Option<T>,
    ) -> (
        io::Result<usize>,
        Vec<T>,
        Option<T>,
    ) {
        self.inner
            .sendmsg_zc(io_slices, socket_addr, msg_control)
            .await
    }

    /// Receives a single datagram message on the socket. On success, returns
    /// the number of bytes read and the origin.
    pub async fn recv_from<T: BoundedBufMut>(
        &self,
        buf: T,
    ) -> crate::BufResult<(usize, SocketAddr), T> {
        self.inner.recv_from(buf).await
    }

    /// Read a packet of data from the socket into the buffer, returning the original buffer and
    /// quantity of data read.
    pub async fn read<T: BoundedBufMut>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.read(buf).await
    }

    /// Like [`read`], but using a pre-mapped buffer
    /// registered with [`FixedBufRegistry`].
    ///
    /// [`read`]: Self::read
    /// [`FixedBufRegistry`]: crate::buf::fixed::FixedBufRegistry
    ///
    /// # Errors
    ///
    /// In addition to errors that can be reported by `read`,
    /// this operation fails if the buffer is not registered in the
    /// current `tokio-uring` runtime.
    pub async fn read_fixed<T>(&self, buf: T) -> crate::BufResult<usize, T>
    where
        T: BoundedBufMut<BufMut = FixedBuf>,
    {
        self.inner.read_fixed(buf).await
    }

    /// Write some data to the socket from the buffer, returning the original buffer and
    /// quantity of data written.
    pub async fn write<T: BoundedBuf>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.write(buf).await
    }

    /// Like [`write`], but using a pre-mapped buffer
    /// registered with [`FixedBufRegistry`].
    ///
    /// [`write`]: Self::write
    /// [`FixedBufRegistry`]: crate::buf::fixed::FixedBufRegistry
    ///
    /// # Errors
    ///
    /// In addition to errors that can be reported by `write`,
    /// this operation fails if the buffer is not registered in the
    /// current `tokio-uring` runtime.
    pub async fn write_fixed<T>(&self, buf: T) -> crate::BufResult<usize, T>
    where
        T: BoundedBuf<Buf = FixedBuf>,
    {
        self.inner.write_fixed(buf).await
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O on the specified portions to return
    /// immediately with an appropriate value.
    pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }
}

impl FromRawFd for UdpSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        UdpSocket::from_socket(Socket::from_shared_fd(SharedFd::new(fd)))
    }
}

impl AsRawFd for UdpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
