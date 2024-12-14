use std::io;

use rustix_uring::{opcode, types};

use crate::{
    io::SharedFd,
    runtime::{
        driver::op::{Completable, CqeResult, Op},
        CONTEXT,
    },
};

pub(crate) struct Fallocate {
    fd: SharedFd,
}

impl Op<Fallocate> {
    pub(crate) fn fallocate(
        fd: &SharedFd,
        offset: u64,
        len: u64,
        flags: i32,
    ) -> io::Result<Op<Fallocate>> {
        CONTEXT.with(|x| {
            x.handle().expect("not in a runtime context").submit_op(
                Fallocate { fd: fd.clone() },
                |fallocate| {
                    opcode::Fallocate::new(types::Fd(fallocate.fd.raw_fd()), len as _)
                        .offset(offset as _)
                        .mode(flags)
                        .build()
                },
            )
        })
    }
}

impl Completable for Fallocate {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
