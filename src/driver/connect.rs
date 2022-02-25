use crate::driver::{Op, SharedFd};
use os_socketaddr::OsSocketAddr;
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
