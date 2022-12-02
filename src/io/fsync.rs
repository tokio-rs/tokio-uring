use std::io;

use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, Op};
use io_uring::{opcode, types};

pub(crate) struct Fsync {
    fd: SharedFd,
}

impl Op<Fsync> {
    pub(crate) fn fsync(fd: &SharedFd) -> io::Result<Op<Fsync>> {
        Op::submit_with(Fsync { fd: fd.clone() }, |fsync| {
            opcode::Fsync::new(types::Fd(fsync.fd.raw_fd())).build()
        })
    }

    pub(crate) fn datasync(fd: &SharedFd) -> io::Result<Op<Fsync>> {
        Op::submit_with(Fsync { fd: fd.clone() }, |fsync| {
            opcode::Fsync::new(types::Fd(fsync.fd.raw_fd()))
                .flags(types::FsyncFlags::DATASYNC)
                .build()
        })
    }
}

impl Completable for Fsync {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
