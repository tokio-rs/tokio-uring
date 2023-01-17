use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, MultiCQEFuture, Op, Updateable};
use crate::runtime::CONTEXT;
use std::io;

pub(crate) struct SendMsgZc {
    #[allow(dead_code)]
    fd: SharedFd,

    pub(crate) msghdr: libc::msghdr,

    /// Hold the number of transmitted bytes
    bytes: usize,
}

impl Op<SendMsgZc, MultiCQEFuture> {
    pub(crate) fn sendmsg_zc(fd: &SharedFd, msghdr: &libc::msghdr) -> io::Result<Self> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                SendMsgZc {
                    fd: fd.clone(),
                    msghdr: (*msghdr).clone(),
                    bytes: 0,
                },
                |sendmsg_zc| {
                    opcode::SendMsgZc::new(
                        types::Fd(sendmsg_zc.fd.raw_fd()),
                        &sendmsg_zc.msghdr as *const _,
                    )
                    .build()
                },
            )
        })
    }
}

impl Completable for SendMsgZc {
    type Output = (libc::msghdr, io::Result<usize>);

    fn complete(self, cqe: CqeResult) -> (libc::msghdr, io::Result<usize>) {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);

        // Recover the msghdr.
        let msghdr = self.msghdr;

        (msghdr, res)
    }
}

impl Updateable for SendMsgZc {
    fn update(&mut self, cqe: CqeResult) {
        // uring send_zc promises there will be no error on CQE's marked more
        self.bytes += *cqe.result.as_ref().unwrap() as usize;
    }
}
