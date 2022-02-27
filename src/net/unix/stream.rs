use std::{io, path::Path};

use crate::{
    buf::{IoBuf, IoBufMut},
    driver::Socket,
};

/// A Unix stream between two local sockets on a Unix OS.
///
/// A Unix stream can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [`accepting`] a connection from a [`listener`].
///
/// # Examples
///
/// ```no_run
/// use tokio_uring::net::UnixStream;
/// use std::net::ToSocketAddrs;
///
/// fn main() -> std::io::Result<()> {
///     tokio_uring::start(async {
///         // Connect to a peer
///         let mut stream = UnixStream::connect("/tmp/tokio-uring-unix-test.sock").await?;
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
/// [`connect`]: UnixStream::connect
/// [`accepting`]: crate::net::TcpListener::accept
/// [`listener`]: crate::net::TcpListener
pub struct UnixStream {
    pub(super) inner: Socket,
}

impl UnixStream {
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
    pub async fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
        let socket = Socket::new_unix(libc::SOCK_STREAM)?;
        socket.connect_unix(path).await?;
        let unix_stream = UnixStream { inner: socket };
        Ok(unix_stream)
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
