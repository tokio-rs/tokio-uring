use crate::buf::bufgroup::BufX;
use crate::io::SharedFd;

use crate::buf::bufring::BufRing;
use crate::runtime::driver::op::{CqeResult, MultiCQEStream, Op, Streamable};
use crate::runtime::CONTEXT;
use std::io;

pub(crate) type RecvMultiStream = Op<RecvMulti, MultiCQEStream>;

pub(crate) struct RecvMulti {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    /// The bufgroup that supplies the bgid and the get_buf function.
    group: BufRing,
}

impl RecvMultiStream {
    pub(crate) fn recv_multi(
        fd: &SharedFd,
        group: BufRing,
        flags: Option<i32>,
    ) -> io::Result<Self> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op_stream(
                    RecvMulti {
                        _fd: fd.clone(),
                        group,
                    },
                    |s| {
                        opcode::RecvMulti::new(types::Fd(fd.raw_fd()), s.group.bgid())
                            .flags(flags.unwrap_or(0))
                            .build()
                    },
                )
        })
    }
}

impl Streamable for RecvMulti {
    type Item = io::Result<BufX>;

    fn stream_next(&self, cqe: CqeResult) -> Self::Item {
        // The kernel is not expected to provide an error when the `more` bit was set,
        // but it's just as easy to handle the case here.
        let res = cqe.result?;
        let flags = cqe.flags;

        // Safety: getting a buffer from the group requires the res and flags values accurately
        // identify a buffer and the length which was written to by the kernel. The res and flags
        // passed here are those provided by the kernel.
        let get_buf_res = unsafe { self.group.get_buf(res, flags) };

        match get_buf_res {
            Ok(Some(bufx)) => Ok(bufx),
            Ok(None) => {
                unreachable!("unexpected combo of 'more', no error, and no buffer");
            }
            Err(e) => Err(e),
        }
    }

    fn stream_complete(self, cqe: CqeResult) -> Option<Self::Item> {
        let res = match cqe.result {
            Ok(res) => res,
            Err(e) => return Some(Err(e)),
        };
        let flags = cqe.flags;

        // Safety: getting a buffer from the group requires the res and flags values accurately
        // identify a buffer and the length which was written to by the kernel. The res and flags
        // passed here are those provided by the kernel.
        let get_buf_res = unsafe { self.group.get_buf(res, flags) };

        match get_buf_res {
            Ok(Some(bufx)) => Some(Ok(bufx)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
