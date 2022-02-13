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
/// ```no_run
/// use tokio_uring::net::TcpListener;
///
/// async fn process_stream<T>(stream: T) {
///     // do work with socket here
/// }
///
/// fn main() {
///     let listener = TcpListener::bind("127.0.0.1:2345".parse().unwrap()).unwrap();
///
///     tokio_uring::start(async move {
///         let (stream, _) = listener.accept().await.unwrap();
///         process_stream(stream).await;
///     });
/// }
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
    /// to this listener. The port allocated can be queried via the `local_addr`
    /// method.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::net::TcpListener;
    ///
    /// async fn process_stream<T>(stream: T) {
    ///     // do work with socket here
    /// }
    ///
    /// fn main() {
    ///     let listener = TcpListener::bind("127.0.0.1:2345".parse().unwrap()).unwrap();
    ///
    ///     tokio_uring::start(async move {
    ///         let (stream, _) = listener.accept().await.unwrap();
    ///         process_stream(stream).await;
    ///     });
    /// }
    /// ```
    pub fn bind(socket_addr: SocketAddr) -> io::Result<TcpListener> {
        let socket = Socket::bind(socket_addr, libc::SOCK_STREAM)?;
        socket.listen(1024)?;
        Ok(TcpListener { inner: socket })
    }

    /// Accepts a new incoming connection from this listener.
    ///
    /// This function will yield once a new TCP connection is established. When
    /// established, the corresponding [`TcpStream`] and the remote peer's
    /// address will be returned.
    ///
    /// [`TcpStream`]: struct@crate::net::TcpStream
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::net::TcpListener;
    ///
    /// async fn process_socket<T>(socket: T) {
    ///     // do work with socket here
    /// }
    ///
    /// fn main() {
    ///     let listener = TcpListener::bind("127.0.0.1:2345".parse().unwrap()).unwrap();
    ///
    ///     tokio_uring::start(async move {
    ///         match listener.accept().await {
    ///             Ok((_socket, addr)) => println!("new client: {:?}", addr),
    ///             Err(e) => println!("couldn't get client: {:?}", e),
    ///         }
    ///     });
    /// }
    /// ```
    pub async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let (socket, socket_addr) = self.inner.accept().await?;
        let stream = TcpStream { inner: socket };
        Ok((stream, socket_addr))
    }
}
