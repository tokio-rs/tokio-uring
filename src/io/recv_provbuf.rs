use crate::buf::bufgroup::BufX;
use crate::io::SharedFd;

use crate::buf::bufring::BufRing;
use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use io_uring::squeue;
use std::io;

pub(crate) struct RecvProvBuf {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// The bufgroup that supplies the bgid and the get_buf function.
    group: BufRing,
}

impl Op<RecvProvBuf> {
    pub(crate) fn recv_provbuf(
        fd: &SharedFd,
        group: BufRing,
        flags: Option<i32>,
    ) -> io::Result<Op<RecvProvBuf>> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                RecvProvBuf {
                    fd: fd.clone(),
                    group,
                },
                |s| {
                    // Get raw buffer info
                    opcode::Recv::new(types::Fd(fd.raw_fd()), std::ptr::null_mut(), 0 as _)
                        .flags(flags.unwrap_or(0))
                        .buf_group(s.group.bgid())
                        .build()
                        .flags(squeue::Flags::BUFFER_SELECT)
                },
            )
        })
    }
}

impl Completable for RecvProvBuf {
    type Output = Result<BufX, io::Error>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        let res = cqe.result?;
        let flags = cqe.flags;

        // Safety: getting a buffer from the group requires the res and flags values accurately
        // indentify a buffer and the length which was written to by the kernel. The res and flags
        // passed here are those provided by the kernel.
        unsafe { self.group.get_buf(res, flags) }
    }
}
