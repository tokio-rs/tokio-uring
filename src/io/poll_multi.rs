use crate::io::SharedFd;

use crate::runtime::driver::op::{CqeResult, MultiCQEStream, Op, Streamable};
use crate::runtime::CONTEXT;
use std::io;

#[allow(dead_code)]
pub(crate) type PollMultiStream = Op<PollMulti, MultiCQEStream>;

pub(crate) struct PollMulti {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,
}

impl PollMultiStream {
    #[allow(dead_code)]
    pub(crate) fn poll_multi(fd: &SharedFd, flags: u32) -> io::Result<Self> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op_stream(PollMulti { _fd: fd.clone() }, |_| {
                    opcode::PollAdd::new(types::Fd(fd.raw_fd()), flags as _)
                        .multi(true)
                        .build()
                })
        })
    }
}

impl Streamable for PollMulti {
    type Item = io::Result<u32>;

    fn stream_next(&self, cqe: CqeResult) -> Self::Item {
        // The kernel is not expected to provide an error when the `more` bit was set,
        // but it's just as easy to handle the case here.
        let res = cqe.result?;
        Ok(res)
    }

    fn stream_complete(self, cqe: CqeResult) -> Option<Self::Item> {
        match cqe.result {
            Ok(res) => Some(Ok(res)),
            Err(e) => Some(Err(e)),
        }
    }
}
