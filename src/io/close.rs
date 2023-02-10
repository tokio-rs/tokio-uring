use crate::io::shared_fd::sealed::CommonFd;
use crate::runtime::driver::op;
use crate::runtime::driver::op::{Completable, Op};
use crate::runtime::CONTEXT;
use std::io;

pub(crate) struct Close {}

impl Op<Close> {
    pub(crate) fn close(fd: CommonFd) -> io::Result<Op<Close>> {
        use io_uring::{opcode, types};

        let (raw, fixed) = match fd {
            CommonFd::Raw(raw) => (raw, None),
            CommonFd::Fixed(fixed) => {
                let fixed = match types::DestinationSlot::try_from_slot_target(fixed) {
                    Ok(n) => n,
                    Err(n) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("invalid fixed slot: {n}"),
                        ))
                    }
                };
                (0, Some(fixed))
            }
        };

        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op(Close {}, |_close| {
                    opcode::Close::new(types::Fd(raw)).file_index(fixed).build()
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
