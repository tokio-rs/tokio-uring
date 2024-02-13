use crate::{
    buf::fixed::FixedBuf,
    buf::{BoundedBuf, BoundedBufMut},
    io::{SharedFd, Socket},
    UnsubmittedWrite,
};
use socket2::SockAddr;
use std::{
    io,
    os::unix::prelude::{AsRawFd, FromRawFd, RawFd},
    path::Path,
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
///         let (result, _) = stream.write(b"hello world!".as_slice()).submit().await;
///         result.unwrap();
///
///         Ok(())
///     })
/// }
/// ```
///
/// [`connect`]: UnixStream::connect
/// [`accepting`]: crate::net::UnixListener::accept
/// [`listener`]: crate::net::UnixListener
pub struct UnixStream {
    pub(super) inner: Socket,
}

impl UnixStream {
    /// Opens a Unix connection to the specified file path. There must be a
    /// `UnixListener` or equivalent listening on the corresponding Unix domain socket
    /// to successfully connect and return a `UnixStream`.
    pub async fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
        let socket = Socket::new_unix(libc::SOCK_STREAM)?;
        socket.connect(SockAddr::unix(path)?).await?;
        let unix_stream = UnixStream { inner: socket };
        Ok(unix_stream)
    }

    /// Creates new `UnixStream` from a previously bound `std::os::unix::net::UnixStream`.
    ///
    /// This function is intended to be used to wrap a TCP stream from the
    /// standard library in the tokio-uring equivalent. The conversion assumes nothing
    /// about the underlying socket; it is left up to the user to decide what socket
    /// options are appropriate for their use case.
    ///
    /// This can be used in conjunction with socket2's `Socket` interface to
    /// configure a socket before it's handed off, such as setting options like
    /// `reuse_address` or binding to multiple addresses.
    pub fn from_std(socket: std::os::unix::net::UnixStream) -> UnixStream {
        let inner = Socket::from_std(socket);
        Self { inner }
    }

    pub(crate) fn from_socket(inner: Socket) -> Self {
        Self { inner }
    }

    /// Read some data from the stream into the buffer, returning the original buffer and
    /// quantity of data read.
    pub async fn read<T: BoundedBufMut>(&self, buf: T) -> crate::Result<usize, T> {
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
    pub async fn read_fixed<T>(&self, buf: T) -> crate::Result<usize, T>
    where
        T: BoundedBufMut<BufMut = FixedBuf>,
    {
        self.inner.read_fixed(buf).await
    }

    /// Write some data to the stream from the buffer, returning the original buffer and
    /// quantity of data written.
    pub fn write<T: BoundedBuf>(&self, buf: T) -> UnsubmittedWrite<T> {
        self.inner.write(buf)
    }

    /// Attempts to write an entire buffer to the stream.
    ///
    /// This method will continuously call [`write`] until there is no more data to be
    /// written or an error is returned. This method will not return until the entire
    /// buffer has been successfully written or an error has occurred.
    ///
    /// If the buffer contains no data, this will never call [`write`].
    ///
    /// # Errors
    ///
    /// This function will return the first error that [`write`] returns.
    ///
    /// [`write`]: Self::write
    pub async fn write_all<T: BoundedBuf>(&self, buf: T) -> crate::Result<(), T> {
        self.inner.write_all(buf).await
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
    pub async fn write_fixed<T>(&self, buf: T) -> crate::Result<usize, T>
    where
        T: BoundedBuf<Buf = FixedBuf>,
    {
        self.inner.write_fixed(buf).await
    }

    /// Attempts to write an entire buffer to the stream.
    ///
    /// This method will continuously call [`write_fixed`] until there is no more data to be
    /// written or an error is returned. This method will not return until the entire
    /// buffer has been successfully written or an error has occurred.
    ///
    /// If the buffer contains no data, this will never call [`write_fixed`].
    ///
    /// # Errors
    ///
    /// This function will return the first error that [`write_fixed`] returns.
    ///
    /// [`write_fixed`]: Self::write
    pub async fn write_fixed_all<T>(&self, buf: T) -> crate::Result<(), T>
    where
        T: BoundedBuf<Buf = FixedBuf>,
    {
        self.inner.write_fixed_all(buf).await
    }

    /// Write data from buffers into this socket returning how many bytes were
    /// written.
    ///
    /// This function will attempt to write the entire contents of `bufs`, but
    /// the entire write may not succeed, or the write may also generate an
    /// error. The bytes will be written starting at the specified offset.
    ///
    /// # Return
    ///
    /// The method returns the operation result and the same array of buffers
    /// passed in as an argument. A return value of `0` typically means that the
    /// underlying socket is no longer able to accept bytes and will likely not
    /// be able to in the future as well, or that the buffer provided is empty.
    ///
    /// # Errors
    ///
    /// Each call to `write` may generate an I/O error indicating that the
    /// operation could not be completed. If an error is returned then no bytes
    /// in the buffer were written to this writer.
    ///
    /// It is **not** considered an error if the entire buffer could not be
    /// written to this writer.
    ///
    /// [`Ok(n)`]: Ok
    pub async fn writev<T: BoundedBuf>(&self, buf: Vec<T>) -> crate::Result<usize, Vec<T>> {
        self.inner.writev(buf).await
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O on the specified portions to return
    /// immediately with an appropriate value.
    pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }
}

impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        UnixStream::from_socket(Socket::from_shared_fd(SharedFd::new(fd)))
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
