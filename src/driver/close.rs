use crate::driver::Op;

use crate::driver::op::{self, Completable};
use std::{io, os::unix::io::RawFd};

pub(crate) struct Close {
    fd: RawFd,
}

impl Op<Close> {
    /// Close a file descriptor
    ///
    /// Close is special, in that it does not wait until it is Polled
    /// before placing the Op on the submission queue. This is to ensure
    /// that if the Driver is Dropped with the Operation incomplete,
    /// the drop logic of the driver will ensure we do not leak resources
    pub(crate) fn close(fd: RawFd) -> io::Result<Op<Close>> {
        use io_uring::{opcode, types};

        let op = Op::submit_with(Close { fd }, |close| {
            opcode::Close::new(types::Fd(close.fd)).build()
        });

        op.enqueue()
    }
}

impl Completable for Close {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        let _ = cqe.result?;
        
        Ok(())
    }
}
