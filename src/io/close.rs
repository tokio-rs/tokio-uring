use crate::runtime::driver::op;
use crate::runtime::driver::op::{Completable, Op};
use crate::runtime::CONTEXT;
use std::io;
use std::os::unix::io::RawFd;

pub(crate) struct Close {
    fd: RawFd,
}

impl Op<Close> {
    pub(crate) fn close(fd: RawFd) -> io::Result<Op<Close>> {
        use rustix_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op(Close { fd }, |close| {
                    opcode::Close::new(types::Fd(close.fd)).build()
                })
        })
    }
}

impl Completable for Close {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        let _ = cqe.result?;

        Ok(())
    }
}
