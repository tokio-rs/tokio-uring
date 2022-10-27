use crate::{
    buf::{IoBuf, IoBufMut},
    driver::{Op, SharedFd},
};
use std::{
    io,
    net::SocketAddr,
    os::unix::io::{AsRawFd, IntoRawFd, RawFd},
    path::Path,
};

#[derive(Clone)]
pub(crate) struct Socket {
    /// Open file descriptor
    pub(crate) fd: SharedFd,
}

pub(crate) fn get_domain(socket_addr: SocketAddr) -> libc::c_int {
    match socket_addr {
        SocketAddr::V4(_) => libc::AF_INET,
        SocketAddr::V6(_) => libc::AF_INET6,
    }
}

impl Socket {
    pub(crate) fn new(socket_addr: SocketAddr, socket_type: libc::c_int) -> io::Result<Socket> {
        let socket_type = socket_type | libc::SOCK_CLOEXEC;
        let domain = get_domain(socket_addr);
        let fd = socket2::Socket::new(domain.into(), socket_type.into(), None)?.into_raw_fd();
        let fd = SharedFd::new(fd);
        Ok(Socket { fd })
    }

    pub(crate) fn new_unix(socket_type: libc::c_int) -> io::Result<Socket> {
        let socket_type = socket_type | libc::SOCK_CLOEXEC;
        let domain = libc::AF_UNIX;
        let fd = socket2::Socket::new(domain.into(), socket_type.into(), None)?.into_raw_fd();
        let fd = SharedFd::new(fd);
        Ok(Socket { fd })
    }

    pub(crate) async fn write<T: IoBuf>(&self, buf: T) -> crate::BufResult<usize, T> {
        let op = Op::write_at(&self.fd, buf, 0).unwrap();
        op.await
    }

    pub async fn writev<T: IoBuf>(&self, buf: Vec<T>) -> crate::BufResult<usize, Vec<T>> {
        let op = Op::writev_at(&self.fd, buf, 0).unwrap();
        op.await
    }

    pub(crate) async fn send_to<T: IoBuf>(
        &self,
        buf: T,
        socket_addr: SocketAddr,
    ) -> crate::BufResult<usize, T> {
        let op = Op::send_to(&self.fd, buf, socket_addr).unwrap();
        op.await
    }

    pub(crate) async fn read<T: IoBufMut>(&self, buf: T) -> crate::BufResult<usize, T> {
        let op = Op::read_at(&self.fd, buf, 0).unwrap();
        op.await
    }

    pub(crate) async fn recv_from<T: IoBufMut>(
        &self,
        buf: T,
    ) -> crate::BufResult<(usize, SocketAddr), T> {
        let op = Op::recv_from(&self.fd, buf).unwrap();
        op.await
    }

    pub(crate) async fn accept(&self) -> io::Result<(Socket, Option<SocketAddr>)> {
        let op = Op::accept(&self.fd)?;
        op.await
    }

    pub(crate) async fn connect(&self, socket_addr: socket2::SockAddr) -> io::Result<()> {
        let op = Op::connect(&self.fd, socket_addr)?;
        op.await
    }

    pub(crate) fn bind(socket_addr: SocketAddr, socket_type: libc::c_int) -> io::Result<Socket> {
        Self::bind_internal(
            socket_addr.into(),
            get_domain(socket_addr).into(),
            socket_type.into(),
        )
    }

    pub(crate) fn bind_unix<P: AsRef<Path>>(
        path: P,
        socket_type: libc::c_int,
    ) -> io::Result<Socket> {
        let addr = socket2::SockAddr::unix(path.as_ref())?;
        Self::bind_internal(addr, libc::AF_UNIX.into(), socket_type.into())
    }

    pub(crate) fn from_std<T: IntoRawFd>(socket: T) -> Socket {
        let fd = SharedFd::new(socket.into_raw_fd());
        Self::from_shared_fd(fd)
    }

    pub(crate) fn from_shared_fd(fd: SharedFd) -> Socket {
        Self { fd }
    }

    fn bind_internal(
        socket_addr: socket2::SockAddr,
        domain: socket2::Domain,
        socket_type: socket2::Type,
    ) -> io::Result<Socket> {
        let sys_listener = socket2::Socket::new(domain, socket_type, None)?;

        sys_listener.set_reuse_port(true)?;
        sys_listener.set_reuse_address(true)?;

        // TODO: config for buffer sizes
        // sys_listener.set_send_buffer_size(send_buf_size)?;
        // sys_listener.set_recv_buffer_size(recv_buf_size)?;

        sys_listener.bind(&socket_addr)?;

        let fd = SharedFd::new(sys_listener.into_raw_fd());

        Ok(Self { fd })
    }

    pub(crate) fn listen(&self, backlog: libc::c_int) -> io::Result<()> {
        syscall!(listen(self.as_raw_fd(), backlog))?;
        Ok(())
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O on the specified portions to return
    /// immediately with an appropriate value.
    pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        use std::os::unix::io::FromRawFd;

        let fd = self.as_raw_fd();
        // SAFETY: Our fd is the handle the kernel has given us for a socket,
        // TCP or Unix, Listener or Stream, so it is a valid file descriptor/socket.
        // Create a socket2::Socket long enough to call its shutdown method
        // and then forget it so the socket is not otherwise dropped here.
        let s = unsafe { socket2::Socket::from_raw_fd(fd) };
        let result = s.shutdown(how);
        std::mem::forget(s);
        result
    }
}

impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}
