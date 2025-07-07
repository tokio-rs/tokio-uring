use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
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
        use rustix_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                Connect {
                    fd: fd.clone(),
                    socket_addr: Box::new(socket_addr),
                },
                |connect| {
                    opcode::Connect::new(
                        types::Fd(connect.fd.raw_fd()),
                        connect.socket_addr.as_ptr() as *const _,
                        connect.socket_addr.len(),
                    )
                    .build()
                },
            )
        })
    }
}

impl Completable for Connect {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
