use std::io;

use io_uring::{opcode, types};

use crate::{
    io::SharedFd,
    runtime::{
        driver::op::{Completable, CqeResult, Op},
        CONTEXT,
    },
};

pub(crate) struct Ftruncate {
    fd: SharedFd,
}

impl Op<Ftruncate> {
    pub(crate) fn ftruncate(fd: &SharedFd, len: u64) -> io::Result<Op<Ftruncate>> {
        CONTEXT.with(|x| {
            x.handle()
                .expect("not in a runtime context")
                .submit_op(Ftruncate { fd: fd.clone() }, |ftruncate| {
                    opcode::Ftruncate::new(types::Fd(ftruncate.fd.raw_fd()), len).build()
                })
        })
    }
}

impl Completable for Ftruncate {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
