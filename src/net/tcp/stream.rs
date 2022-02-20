use std::{io, net::SocketAddr};

use crate::{
    buf::{IoBuf, IoBufMut},
    driver::Socket,
};

/// A TCP stream between a local and a remote socket.
///
/// A TCP stream can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [`accepting`] a connection from a [`listener`].
///
/// # Examples
///
/// ```no_run
/// use tokio_uring::net::TcpStream;
/// use std::net::ToSocketAddrs;
///
/// fn main() -> std::io::Result<()> {
///     tokio_uring::start(async {
///         // Connect to a peer
///         let mut stream = TcpStream::connect(
///             "127.0.0.1:8080".to_socket_addrs().unwrap().next().unwrap()
///         ).await?;
///
///         // Write some data.
///         let (result, _) = stream.write(b"hello world!".as_slice()).await;
///         result.unwrap();
///
///         Ok(())
///     })
/// }
/// ```
///
/// [`connect`]: TcpStream::connect
/// [`accepting`]: crate::net::TcpListener::accept
/// [`listener`]: crate::net::TcpListener
pub struct TcpStream {
    pub(super) inner: Socket,
}

impl TcpStream {
    /// Opens a TCP connection to a remote host.
    ///
    /// `addr` is an address of the remote host. Anything which implements the
    /// [`ToSocketAddrs`] trait can be supplied as the address.  If `addr`
    /// yields multiple addresses, connect will be attempted with each of the
    /// addresses until a connection is successful. If none of the addresses
    /// result in a successful connection, the error returned from the last
    /// connection attempt (the last address) is returned.
    ///
    /// [`ToSocketAddrs`]: trait@tokio::net::ToSocketAddrs
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::net::TcpStream;
    /// use std::net::ToSocketAddrs;
    /// fn main() -> std::io::Result<()> {
    ///     tokio_uring::start(async {
    ///         // Connect to a peer
    ///         let mut stream = TcpStream::connect(
    ///             "127.0.0.1:8080".to_socket_addrs().unwrap().next().unwrap()
    ///         ).await?;
    ///
    ///         // Write some data.
    ///         let (result, _) = stream.write(b"hello world!".as_slice()).await;
    ///         let written = result.unwrap();
    ///
    ///         println!("written: {}", written);
    ///
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub async fn connect(socket_addr: SocketAddr) -> io::Result<TcpStream> {
        let socket = Socket::new(socket_addr, libc::SOCK_STREAM)?;
        socket.connect(socket_addr).await?;
        let tcp_stream = TcpStream { inner: socket };
        Ok(tcp_stream)
    }

    /// Read some data from the stream into the buffer, returning the original buffer and
    /// quantity of data read.
    pub async fn read<T: IoBufMut>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.read(buf).await
    }

    /// Write some data to the stream from the buffer, returning the original buffer and
    /// quantity of data written.
    pub async fn write<T: IoBuf>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.write(buf).await
    }
}
