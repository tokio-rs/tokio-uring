use crate::io::shared_fd::sealed::CommonFd;
use crate::runtime::driver::op;
use crate::runtime::driver::op::{Completable, Op};
use crate::runtime::CONTEXT;
use std::io;

pub(crate) struct Close {}

impl Op<Close> {
    pub(crate) fn close(fd: CommonFd) -> io::Result<Op<Close>> {
        use io_uring::{opcode, types};

        match fd {
            CommonFd::Raw(raw) => {
                let fd = types::Fd(raw);
                CONTEXT.with(|x| {
                    x.handle()
                        .expect("Not in a runtime context")
                        .submit_op(Close {}, |_close| opcode::Close::new(fd).build())
                })
            }
            CommonFd::Fixed(fixed) => {
                let fd = types::Fixed(fixed);
                CONTEXT.with(|x| {
                    x.handle()
                        .expect("Not in a runtime context")
                        .submit_op(Close {}, |_close| opcode::Close::new(fd).build())
                })
            }
        }
    }
}

impl Completable for Close {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        let _ = cqe.result?;

        Ok(())
    }
}
