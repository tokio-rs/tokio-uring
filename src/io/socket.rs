use crate::io::write::UnsubmittedWrite;
use crate::runtime::driver::op::Op;
use crate::{
    buf::fixed::FixedBuf,
    buf::{BoundedBuf, BoundedBufMut, IoBuf, Slice},
    io::SharedFd,
    UnsubmittedOneshot,
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

    pub(crate) fn write<T: BoundedBuf>(&self, buf: T) -> UnsubmittedWrite<T> {
        UnsubmittedOneshot::write_at(&self.fd, buf, 0)
    }

    pub async fn write_all<T: BoundedBuf>(&self, buf: T) -> crate::BufResult<(), T> {
        let orig_bounds = buf.bounds();
        let (res, buf) = self.write_all_slice(buf.slice_full()).await;
        (res, T::from_buf_bounds(buf, orig_bounds))
    }

    async fn write_all_slice<T: IoBuf>(&self, mut buf: Slice<T>) -> crate::BufResult<(), T> {
        while buf.bytes_init() != 0 {
            let res = self.write(buf).submit().await;
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
                (Ok(n), slice) => {
                    buf = slice.slice(n..);
                }

                // No match on an EINTR error is performed because this
                // crate's design ensures we are not calling the 'wait' option
                // in the ENTER syscall. Only an Enter with 'wait' can generate
                // an EINTR according to the io_uring man pages.
                (Err(e), slice) => return (Err(e), slice.into_inner()),
            }
        }

        (Ok(()), buf.into_inner())
    }

    pub(crate) async fn write_fixed<T>(&self, buf: T) -> crate::BufResult<usize, T>
    where
        T: BoundedBuf<Buf = FixedBuf>,
    {
        let op = Op::write_fixed_at(&self.fd, buf, 0).unwrap();
        op.await
    }

    pub(crate) async fn write_fixed_all<T>(&self, buf: T) -> crate::BufResult<(), T>
    where
        T: BoundedBuf<Buf = FixedBuf>,
    {
        let orig_bounds = buf.bounds();
        let (res, buf) = self.write_fixed_all_slice(buf.slice_full()).await;
        (res, T::from_buf_bounds(buf, orig_bounds))
    }

    async fn write_fixed_all_slice(
        &self,
        mut buf: Slice<FixedBuf>,
    ) -> crate::BufResult<(), FixedBuf> {
        while buf.bytes_init() != 0 {
            let res = self.write_fixed(buf).await;
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
                (Ok(n), slice) => {
                    buf = slice.slice(n..);
                }

                // No match on an EINTR error is performed because this
                // crate's design ensures we are not calling the 'wait' option
                // in the ENTER syscall. Only an Enter with 'wait' can generate
                // an EINTR according to the io_uring man pages.
                (Err(e), slice) => return (Err(e), slice.into_inner()),
            }
        }

        (Ok(()), buf.into_inner())
    }

    pub async fn writev<T: BoundedBuf>(&self, buf: Vec<T>) -> crate::BufResult<usize, Vec<T>> {
        let op = Op::writev_at(&self.fd, buf, 0).unwrap();
        op.await
    }

    pub(crate) async fn send_to<T: BoundedBuf>(
        &self,
        buf: T,
        socket_addr: Option<SocketAddr>,
    ) -> crate::BufResult<usize, T> {
        let op = Op::send_to(&self.fd, buf, socket_addr).unwrap();
        op.await
    }

    pub(crate) async fn send_zc<T: BoundedBuf>(&self, buf: T) -> crate::BufResult<usize, T> {
        let op = Op::send_zc(&self.fd, buf).unwrap();
        op.await
    }

    pub(crate) async fn sendmsg<T: BoundedBuf, U: BoundedBuf>(
        &self,
        io_slices: Vec<T>,
        socket_addr: Option<SocketAddr>,
        msg_control: Option<U>,
    ) -> (io::Result<usize>, Vec<T>, Option<U>) {
        let op = Op::sendmsg(&self.fd, io_slices, socket_addr, msg_control).unwrap();
        op.await
    }

    pub(crate) async fn sendmsg_zc<T: BoundedBuf, U: BoundedBuf>(
        &self,
        io_slices: Vec<T>,
        socket_addr: Option<SocketAddr>,
        msg_control: Option<U>,
    ) -> (io::Result<usize>, Vec<T>, Option<U>) {
        let op = Op::sendmsg_zc(&self.fd, io_slices, socket_addr, msg_control).unwrap();
        op.await
    }

    pub(crate) async fn read<T: BoundedBufMut>(&self, buf: T) -> crate::BufResult<usize, T> {
        let op = Op::read_at(&self.fd, buf, 0).unwrap();
        op.await
    }

    pub(crate) async fn read_fixed<T>(&self, buf: T) -> crate::BufResult<usize, T>
    where
        T: BoundedBufMut<BufMut = FixedBuf>,
    {
        let op = Op::read_fixed_at(&self.fd, buf, 0).unwrap();
        op.await
    }

    pub(crate) async fn recv_from<T: BoundedBufMut>(
        &self,
        buf: T,
    ) -> crate::BufResult<(usize, SocketAddr), T> {
        let op = Op::recv_from(&self.fd, buf).unwrap();
        op.await
    }

    pub(crate) async fn recvmsg<T: BoundedBufMut>(
        &self,
        buf: Vec<T>,
    ) -> crate::BufResult<(usize, SocketAddr), Vec<T>> {
        let op = Op::recvmsg(&self.fd, buf).unwrap();
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
            true,
        )
    }

    pub(crate) fn bind_unix<P: AsRef<Path>>(
        path: P,
        socket_type: libc::c_int,
    ) -> io::Result<Socket> {
        let addr = socket2::SockAddr::unix(path.as_ref())?;
        Self::bind_internal(addr, libc::AF_UNIX.into(), socket_type.into(), false)
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
        reuse_port: bool,
    ) -> io::Result<Socket> {
        let sys_listener = socket2::Socket::new(domain, socket_type, None)?;

        if reuse_port {
            // linux 6.12.9+ raises an error when this option is used
            // on an unsupported socket type instead of ignoring it
            sys_listener.set_reuse_port(true)?;
        }
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
        let socket_ref = socket2::SockRef::from(self);
        socket_ref.shutdown(how)
    }

    /// Set the value of the `TCP_NODELAY` option on this socket.
    ///
    /// If set, this option disables the Nagle algorithm. This means that
    /// segments are always sent as soon as possible, even if there is only a
    /// small amount of data. When not set, data is buffered until there is a
    /// sufficient amount to send out, thereby avoiding the frequent sending of
    /// small packets.
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        let socket_ref = socket2::SockRef::from(self);
        socket_ref.set_nodelay(nodelay)
    }
}

impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}
