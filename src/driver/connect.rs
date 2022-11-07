use crate::driver::op::{self, Buildable, Completable};
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
        Connect {
            fd: fd.clone(),
            socket_addr: Box::new(socket_addr),
        }
        .submit()
    }
}

impl Buildable for Connect
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};

        opcode::Connect::new(
            types::Fd(self.fd.raw_fd()),
            self.socket_addr.as_ptr(),
            self.socket_addr.len(),
        )
        .build()
    }
}

impl Completable for Connect {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
