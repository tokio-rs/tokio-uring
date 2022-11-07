use crate::driver::{Op, SharedFd};

use std::io;

use crate::driver::op::{self, Buildable, Completable};
use io_uring::{opcode, types};

pub(crate) struct Fsync {
    fd: SharedFd,
    flags: Option<types::FsyncFlags>,
}

impl Op<Fsync> {
    pub(crate) fn fsync(fd: &SharedFd) -> io::Result<Op<Fsync>> {
        Fsync {
            fd: fd.clone(),
            flags: None,
        }
        .submit()
    }

    pub(crate) fn datasync(fd: &SharedFd) -> io::Result<Op<Fsync>> {
        Fsync {
            fd: fd.clone(),
            flags: Some(types::FsyncFlags::DATASYNC),
        }
        .submit()
    }
}

impl Buildable for Fsync
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        let mut opcode = opcode::Fsync::new(types::Fd(self.fd.raw_fd()));

        if let Some(flags) = self.flags {
            opcode.flags(flags);
        }

        opcode.build()
    }
}

impl Completable for Fsync {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
