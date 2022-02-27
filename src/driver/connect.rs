use crate::driver::{Op, SharedFd};
use os_socketaddr::OsSocketAddr;
use socket2::SockAddr;
use std::{io, net::SocketAddr};

/// Open a file
pub(crate) struct Connect {
    fd: SharedFd,
    os_socket_addr: OsSocketAddr,
}

impl Op<Connect> {
    /// Submit a request to connect.
    pub(crate) fn connect(fd: &SharedFd, socket_addr: SocketAddr) -> io::Result<Op<Connect>> {
        use io_uring::{opcode, types};

        let os_socket_addr = OsSocketAddr::from(socket_addr);

        Op::submit_with(
            Connect {
                fd: fd.clone(),
                os_socket_addr,
            },
            |connect| {
                opcode::Connect::new(
                    types::Fd(connect.fd.raw_fd()),
                    connect.os_socket_addr.as_ptr(),
                    connect.os_socket_addr.len(),
                )
                .build()
            },
        )
    }
}

pub(crate) struct ConnectUnix {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    pub(crate) fd: SharedFd,
    socket_addr: SockAddr,
}

impl Op<ConnectUnix> {
    /// Submit a request to connect.
    pub(crate) fn connect_unix(
        fd: &SharedFd,
        socket_addr: SockAddr,
    ) -> io::Result<Op<ConnectUnix>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            ConnectUnix {
                fd: fd.clone(),
                socket_addr,
            },
            |connect| {
                opcode::Connect::new(
                    types::Fd(connect.fd.raw_fd()),
                    connect.socket_addr.as_ptr(),
                    connect.socket_addr.len(),
                )
                .build()
            },
        )
    }
}
