use crate::driver::Op;

use crate::driver::op::{self, Buildable, Completable};
use std::io;
use std::os::unix::io::RawFd;

pub(crate) struct Close {
    fd: RawFd,
}

impl Op<Close> {
    pub(crate) fn close(fd: RawFd) -> io::Result<Op<Close>> {
        Close { fd }.submit()
    }
}

impl Buildable for Close
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};
        opcode::Close::new(types::Fd(self.fd)).build()
    }
}

impl Completable for Close {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        let _ = cqe.result?;

        Ok(())
    }
}
