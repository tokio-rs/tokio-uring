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
            CommonFd::Raw(raw) => CONTEXT.with(|x| {
                x.handle()
                    .expect("Not in a runtime context")
                    .submit_op(Close {}, |_close| {
                        opcode::Close::new(types::Fd(raw)).build()
                    })
            }),
            CommonFd::Fixed(_fixed) => {
                /* TODO not yet implemented by io-uring
                CONTEXT.with(|x| {
                    x.handle()
                        .expect("Not in a runtime context")
                        .submit_op(Close { fd }, |_close| {
                            opcode::Close_direct::new(_fixed).build()
                        })
                })
                */
                unreachable!("waiting for the ability to create a fixed descriptor for a request");
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
