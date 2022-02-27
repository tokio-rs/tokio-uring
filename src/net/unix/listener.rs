use super::UnixStream;
use crate::driver::Socket;
use std::{io, path::Path};

/// A Unix socket server, listening for connections.
///
/// You can accept a new connection by using the [`accept`](`UnixListener::accept`)
/// method.
///
/// # Examples
///
/// ```
/// use tokio_uring::net::UnixListener;
/// use tokio_uring::net::UnixStream;
///
/// fn main() {
///     let listener = UnixListener::bind("127.0.0.1:2345".parse().unwrap()).unwrap();
///
///     tokio_uring::start(async move {
///         let tx_fut = UnixStream::connect("127.0.0.1:2345".parse().unwrap());
///
///         let rx_fut = listener.accept();
///
///         let (tx, (rx, _)) = tokio::try_join!(tx_fut, rx_fut).unwrap();
///
///         tx.write(b"test" as &'static [u8]).await.0.unwrap();
///
///         let (_, buf) = rx.read(vec![0; 4]).await;
///
///         assert_eq!(buf, b"test");
///     });
/// }
/// ```
pub struct UnixListener {
    inner: Socket,
}

impl UnixListener {
    /// Creates a new UnixListener, which will be bound to the specified address.
    ///
    /// The returned listener is ready for accepting connections.
    ///
    /// Binding with a port number of 0 will request that the OS assigns a port
    /// to this listener. The port allocated can be queried via the `local_addr`
    /// method.
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        let socket = Socket::bind_unix(path, libc::SOCK_STREAM)?;
        socket.listen(1024)?;
        Ok(UnixListener { inner: socket })
    }

    /// Accepts a new incoming connection from this listener.
    ///
    /// This function will yield once a new TCP connection is established. When
    /// established, the corresponding [`UnixStream`] and the remote peer's
    /// address will be returned.
    ///
    /// [`UnixStream`]: struct@crate::net::UnixStream
    pub async fn accept(&self) -> io::Result<UnixStream> {
        let socket = self.inner.accept_unix().await?;
        let stream = UnixStream { inner: socket };
        Ok(stream)
    }
}
