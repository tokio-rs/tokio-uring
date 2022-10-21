use std::{
    io,
    net::SocketAddr,
    os::unix::prelude::{AsRawFd, FromRawFd, RawFd},
};

use crate::bufgroup::{self, BufX};
use crate::{
    buf::{IoBuf, IoBufMut},
    driver::{SharedFd, Socket},
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
///         let mut stream = TcpStream::connect("127.0.0.1:8080".parse().unwrap()).await?;
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
    /// Opens a TCP connection to a remote host at the given `SocketAddr`
    pub async fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
        let socket = Socket::new(addr, libc::SOCK_STREAM)?;
        socket.connect(socket2::SockAddr::from(addr)).await?;
        let tcp_stream = TcpStream { inner: socket };
        Ok(tcp_stream)
    }

    pub(crate) fn from_socket(inner: Socket) -> Self {
        Self { inner }
    }

    /// Read some data from the stream into the buffer, returning the original buffer and
    /// quantity of data read.
    pub async fn read<T: IoBufMut>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.read(buf).await
    }

    /// Read some data from the stream into a buffer from the buffer group, returning the buffer
    /// that the kernel selected. The quantity of data read can be inferred from the length of the
    /// buffer.
    ///
    /// When the buffer being returned goes out of scope, it is returned to the buffer group for
    /// the kernel to reuse until such time that the buffer group is unregistered or the uring
    /// interface is shutdown.
    ///
    /// Holding the returned buffer may lead to starvation of other read or receiver requests that
    /// are programmed to use the same buffer group.
    pub async fn read_bg<G>(&self, buf_group: G) -> io::Result<BufX<G>>
    where
        G: Clone + bufgroup::Group + std::marker::Unpin + 'static,
    {
        self.inner.read_bg(buf_group).await
    }

    /// Write some data to the stream from the buffer, returning the original buffer and
    /// quantity of data written.
    pub async fn write<T: IoBuf>(&self, buf: T) -> crate::BufResult<usize, T> {
        self.inner.write(buf).await
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
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::SocketAddr;
    /// use tokio_uring::net::TcpListener;
    /// use tokio_uring::buf::IoBuf;
    ///
    /// let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    ///
    /// tokio_uring::start(async {
    ///     let listener = TcpListener::bind(addr).unwrap();
    ///
    ///     println!("Listening on {}", listener.local_addr().unwrap());
    ///
    ///     loop {
    ///         let (stream, _) = listener.accept().await.unwrap();
    ///         tokio_uring::spawn(async move {
    ///             let mut n = 0;
    ///             let mut buf = vec![0u8; 4096];
    ///             loop {
    ///                 let (result, nbuf) = stream.read(buf).await;
    ///                 buf = nbuf;
    ///                 let read = result.unwrap();
    ///                 if read == 0 {
    ///                     break;
    ///                 }
    ///
    ///                 let (res, slice) = stream.write_all(buf.slice(..read)).await;
    ///                 let _ = res.unwrap();
    ///                 buf = slice.into_inner();
    ///                 n += read;
    ///             }
    ///         });
    ///     }
    /// });
    /// ```
    ///
    /// [`write`]: Self::write
    pub async fn write_all<T: IoBuf>(&self, mut buf: T) -> crate::BufResult<(), T> {
        let mut n = 0;
        while n < buf.bytes_init() {
            let res = self.write(buf.slice(n..)).await;
            match res {
                (Ok(0), slice) => {
                    return (
                        Err(std::io::Error::new(
                            std::io::ErrorKind::WriteZero,
                            "failed to write whole buffer",
                        )),
                        slice.into_inner(),
                    )
                }
                (Ok(m), slice) => {
                    n += m;
                    buf = slice.into_inner();
                }

                // This match on an EINTR error is not performed because this
                // crate's design ensures we are not calling the 'wait' option
                // in the ENTER syscall. Only an Enter with 'wait' can generate
                // an EINTR according to the io_uring man pages.
                // (Err(ref e), slice) if e.kind() == std::io::ErrorKind::Interrupted => {
                //     buf = slice.into_inner();
                // },
                (Err(e), slice) => return (Err(e), slice.into_inner()),
            }
        }

        (Ok(()), buf)
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O on the specified portions to return
    /// immediately with an appropriate value.
    pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        // TODO same method for unix stream for consistency.
        self.inner.shutdown(how)
    }

    /// Returns the socket address of the remote peer of this TCP connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        let fd = self.inner.as_raw_fd();
        // SAFETY: Our fd is the handle the kernel has given us for a TcpListener.
        // Create a std::net::TcpListener long enough to call its peer_addr method
        // and then forget it so the socket is not closed here.
        let s = unsafe { std::net::TcpStream::from_raw_fd(fd) };
        let peer_addr = s.peer_addr();
        std::mem::forget(s);
        peer_addr
    }
}

impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        TcpStream::from_socket(Socket::from_shared_fd(SharedFd::new(fd)))
    }
}

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
