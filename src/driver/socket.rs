use crate::{
    buf::{IoBuf, IoBufMut},
    driver::{Op, SharedFd},
};
use socket2::SockAddr;
use std::{
    io,
    net::SocketAddr,
    os::unix::io::{AsRawFd, RawFd},
    path::Path,
};

#[derive(Clone)]
pub(crate) struct Socket {
    /// Open file descriptor
    fd: SharedFd,
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
        let fd = syscall!(socket(domain, socket_type, 0))?;
        let fd = SharedFd::new(fd);
        Ok(Socket { fd })
    }

    pub(crate) fn new_unix(socket_type: libc::c_int) -> io::Result<Socket> {
        let socket_type = socket_type | libc::SOCK_CLOEXEC;
        let domain = libc::AF_UNIX;
        let fd = syscall!(socket(domain, socket_type, 0))?;
        let fd = SharedFd::new(fd);
        Ok(Socket { fd })
    }

    pub(crate) async fn write<T: IoBuf>(&self, buf: T) -> crate::BufResult<usize, T> {
        let op = Op::write_at(&self.fd, buf, 0).unwrap();
        op.write().await
    }

    pub(crate) async fn send_to<T: IoBuf>(
        &self,
        buf: T,
        socket_addr: SocketAddr,
    ) -> crate::BufResult<usize, T> {
        let op = Op::send_to(&self.fd, buf, socket_addr).unwrap();
        op.send().await
    }

    pub(crate) async fn read<T: IoBufMut>(&self, buf: T) -> crate::BufResult<usize, T> {
        let op = Op::read_at(&self.fd, buf, 0).unwrap();
        op.read().await
    }

    pub(crate) async fn recv_from<T: IoBufMut>(
        &self,
        buf: T,
    ) -> crate::BufResult<(usize, SocketAddr), T> {
        let op = Op::recv_from(&self.fd, buf).unwrap();
        op.recv().await
    }

    pub(crate) async fn accept(&self) -> io::Result<(Socket, Option<SocketAddr>)> {
        let op = Op::accept(&self.fd)?;
        let completion = op.await;
        let fd = completion.result?;
        let fd = SharedFd::new(fd as i32);
        let data = completion.data;
        let socket = Socket { fd };
        let (_, addr) = unsafe {
            SockAddr::init(move |addr_storage, len| {
                *addr_storage = data.socketaddr.0.to_owned();
                *len = data.socketaddr.1;
                Ok(())
            })?
        };
        Ok((socket, addr.as_socket()))
    }

    pub(crate) async fn connect(&self, socket_addr: SockAddr) -> io::Result<()> {
        let op = Op::connect(&self.fd, socket_addr)?;
        let completion = op.await;
        completion.result?;
        Ok(())
    }

    pub(crate) fn bind(socket_addr: SocketAddr, socket_type: libc::c_int) -> io::Result<Socket> {
        let socket = Socket::new(socket_addr, socket_type)?;
        let addr = SockAddr::from(socket_addr);
        syscall!(bind(socket.as_raw_fd(), addr.as_ptr(), addr.len()))?;
        Ok(socket)
    }

    pub(crate) fn bind_unix<P: AsRef<Path>>(
        path: P,
        socket_type: libc::c_int,
    ) -> io::Result<Socket> {
        let socket = Socket::new_unix(socket_type)?;
        let addr = SockAddr::unix(path.as_ref())?;
        syscall!(bind(socket.as_raw_fd(), addr.as_ptr(), addr.len()))?;
        Ok(socket)
    }

    pub(crate) fn listen(&self, backlog: libc::c_int) -> io::Result<()> {
        syscall!(listen(self.as_raw_fd(), backlog))?;
        Ok(())
    }
}

impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}
