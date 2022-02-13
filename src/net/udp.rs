use crate::{
    buf::{IoBuf, IoBufMut},
    driver::Socket,
};
use std::{io, net::SocketAddr};

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
///
/// ```no_run
/// use tokio_uring::net::UdpSocket;
///
/// fn main() {
///     tokio_uring::start(async {
///         // Connect to a peer
///         let mut stream = UdpSocket::connect("127.0.0.1:8080").await?;
///
///         // Write some data.
///         let (result, _) = stream.write(b"hello world!").await;
///         result.unwrap();
///
///         Ok(())
///     });
/// }
/// ```
pub struct UdpSocket {
    pub(super) inner: Socket,
}

impl UdpSocket {
    /// This function will create a new UDP socket and attempt to bind it to the addr provided.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::net::UdpSocket;
    ///
    /// fn main() {
    ///     tokio_uring::start(async {
    ///         // Connect to a peer
    ///         let socket = UdpSocket::bind("127.0.0.1:0").await?;
    ///
    ///         // Write some data.
    ///         let (result, _) = stream.send_to(b"hello world!").await;
    ///         let written = result.unwrap();
    ///
    ///         println!("written: {}", written);
    ///
    ///         Ok(())
    ///     });
    /// }
    /// ```
    pub async fn bind(socket_addr: SocketAddr) -> io::Result<UdpSocket> {
        let socket = Socket::bind(socket_addr, libc::SOCK_DGRAM)?;
        Ok(UdpSocket { inner: socket })
    }

    /// Connects this UDP socket to a remote address, allowing the `write` and
    /// `read` syscalls to be used to send data and also applies filters to only
    /// receive data from the specified address.
    ///
    /// Note that usually, a successful `connect` call does not specify
    /// that there is a remote server listening on the port, rather, such an
    /// error would only be detected after the first send.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::net::UdpSocket;
    ///
    /// fn main() {
    ///     tokio_uring::start(async {
    ///         // Connect to a peer
    ///         let socket = UdpSocket::bind("127.0.0.1:0".parse().unwrap()).await?;
    ///
    ///         socket.connect("127.0.0.1:8080".parse().unwrap()).await.unwrap();
    ///
    ///         // Send some data to 127.0.0.1:8080.
    ///         let (result, _) = stream.write(b"hello world!").await;
    ///         let written = result.unwrap();
    ///
    ///         println!("sent: {}", written);
    ///
    ///         Ok(())
    ///     });
    /// }
    /// ```
    pub async fn connect(&self, socket_addr: SocketAddr) -> io::Result<()> {
        self.inner.connect(socket_addr).await
    }

    /// Sends data on the socket to the given address. On success, returns the
    /// number of bytes written.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::net::UdpSocket;
    ///
    /// fn main() {
    ///     tokio_uring::start(async {
    ///         // Connect to a peer
    ///         let socket = UdpSocket::bind("127.0.0.1:0".parse().unwrap()).await?;
    ///
    ///         // Send some data to 127.0.0.1:8080.
    ///         let (result, _) = stream.send(b"hello world!",
    ///                                       "127.0.0.1:8080".parse().unwrap()).await;
    ///         let sent = result.unwrap();
    ///
    ///         println!("sent: {}", sent);
    ///
    ///         Ok(())
    ///     });
    /// }
    /// ```
    pub async fn send_to<T: IoBuf>(
        &self,
        buf: T,
        socket_addr: SocketAddr,
    ) -> crate::BufResult<usize, T> {
        self.inner.send_to(buf, socket_addr).await
    }

    /// Receives a single datagram message on the socket. On success, returns
    /// the number of bytes read and the origin.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::net::UdpSocket;
    ///
    /// fn main() {
    ///     tokio_uring::start(async {
    ///         // Connect to a peer
    ///         let socket = UdpSocket::bind("127.0.0.1:0".parse().unwrap()).await?;
    ///
    ///         // Send some data to 127.0.0.1:8080.
    ///         let (result, _) = stream.recv_from(b"hello world!").await;
    ///         let (received, addr) = result.unwrap();
    ///
    ///         println!("received from {}: {}", addr, received);
    ///
    ///         Ok(())
    ///     });
    /// }
    /// ```
    pub async fn recv_from<T: IoBufMut>(&self, buf: T) -> crate::BufResult<(usize, SocketAddr), T> {
        self.inner.recv_from(buf).await
    }

    /// Read a packet of data from the socket into the buffer, returning the original buffer and
    /// quantity of data read.
    pub async fn read<T: IoBufMut>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.read(buf).await
    }

    /// Write some data to the socket from the buffer, returning the original buffer and
    /// quantity of data written.
    pub async fn write<T: IoBuf>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.write(buf).await
    }
}
