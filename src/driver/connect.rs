use crate::driver::op::{self, Completable};
use crate::driver::{Op, SharedFd};
use socket2::SockAddr;
use std::io;

/// Open a file
pub(crate) struct Connect {
    fd: SharedFd,
    // this avoids a UAF (UAM?) if the future is moved, but not if the future is
    // dropped. no Op can be dropped before completion in tokio-uring land right now.
    socket_addr: Box<SockAddr>,
}

impl Op<Connect> {
    /// Submit a request to connect.
    pub(crate) fn connect(fd: &SharedFd, socket_addr: SockAddr) -> io::Result<Op<Connect>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            Connect {
                fd: fd.clone(),
                socket_addr: Box::new(socket_addr),
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

impl Completable for Connect {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
