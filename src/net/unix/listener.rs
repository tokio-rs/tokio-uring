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
/// let sock_file = "/tmp/tokio-uring-unix-test.sock";
/// let listener = UnixListener::bind(&sock_file).unwrap();
///
/// tokio_uring::start(async move {
///     let (tx_ch, rx_ch) = tokio::sync::oneshot::channel();
///
///     tokio_uring::spawn(async move {
///         let rx = listener.accept().await.unwrap();
///         if let Err(_) = tx_ch.send(rx) {
///             panic!("The receiver dropped");
///         }
///     });
///     tokio::task::yield_now().await; // Ensure the listener.accept().await has been kicked off.
///
///     let tx = UnixStream::connect(&sock_file).await.unwrap();
///     let rx = rx_ch.await.expect("The spawned task expected to send a UnixStream");
///
///     tx.write(b"test" as &'static [u8]).await.0.unwrap();
///
///     let (_, buf) = rx.read(vec![0; 4]).await;
///
///     assert_eq!(buf, b"test");
/// });
///
/// std::fs::remove_file(&sock_file).unwrap();
/// ```
pub struct UnixListener {
    inner: Socket,
}

impl UnixListener {
    /// Creates a new UnixListener, which will be bound to the specified file path.
    /// The file path cannnot yet exist, and will be cleaned up upon dropping `UnixListener`
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        let socket = Socket::bind_unix(path, libc::SOCK_STREAM)?;
        socket.listen(1024)?;
        Ok(UnixListener { inner: socket })
    }

    /// Returns the local address that this listener is bound to.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::net::UnixListener;
    /// use std::path::Path;
    ///
    /// let sock_file = "/tmp/tokio-uring-unix-test.sock";
    /// let listener = UnixListener::bind(&sock_file).unwrap();
    ///
    /// let addr = listener.local_addr().expect("Couldn't get local address");
    /// assert_eq!(addr.as_pathname(), Some(Path::new(sock_file)));
    ///
    /// std::fs::remove_file(&sock_file).unwrap();
    /// ```
    pub fn local_addr(&self) -> io::Result<std::os::unix::net::SocketAddr> {
        use std::os::unix::io::{AsRawFd, FromRawFd};

        let fd = self.inner.as_raw_fd();
        // SAFETY: Our fd is the handle the kernel has given us for a UnixListener.
        // Create a std::net::UnixListener long enough to call its local_addr method
        // and then forget it so the socket is not closed here.
        let l = unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) };
        let local_addr = l.local_addr();
        std::mem::forget(l);
        local_addr
    }

    /// Accepts a new incoming connection from this listener.
    ///
    /// This function will yield once a new Unix domain socket connection
    /// is established. When established, the corresponding [`UnixStream`] and
    /// will be returned.
    ///
    /// [`UnixStream`]: struct@crate::net::UnixStream
    pub async fn accept(&self) -> io::Result<UnixStream> {
        let (socket, _) = self.inner.accept().await?;
        let stream = UnixStream { inner: socket };
        Ok(stream)
    }
}
