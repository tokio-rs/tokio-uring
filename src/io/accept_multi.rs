use crate::io::{SharedFd, Socket};

use crate::runtime::driver::op::{CqeResult, MultiCQEStream, Op, Streamable};
use crate::runtime::CONTEXT;
use std::io;

pub(crate) type AcceptMultiStream = Op<AcceptMulti, MultiCQEStream>;

pub(crate) struct AcceptMulti {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,
}

impl AcceptMultiStream {
    /// Valid flags are:
    ///
    /// SOCK_NONBLOCK   Set the O_NONBLOCK file status flag on the open file description
    ///                 (see open(2)) referred to by the new file descriptor. Using this
    ///                 flag saves extra calls to fcntl(2) to achieve the same re sult.
    ///
    /// SOCK_CLOEXEC    Set the close-on-exec (FD_CLOEXEC) flag on the new file descriptor.
    ///                 See the description of the O_CLOEXEC flag in open(2) for
    ///                 reasons why this may be useful.
    pub(crate) fn accept_multi(fd: &SharedFd, flags: Option<i32>) -> io::Result<Self> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op_stream(AcceptMulti { _fd: fd.clone() }, |s| {
                    opcode::AcceptMulti::new(types::Fd(s._fd.raw_fd()))
                        .flags(flags.unwrap_or(0))
                        .build()
                })
        })
    }
}

impl Streamable for AcceptMulti {
    type Item = io::Result<Socket>;

    fn stream_next(&self, cqe: CqeResult) -> Self::Item {
        // The kernel is not expected to provide an error when the `more` bit was set,
        // but it's just as easy to handle the case here.
        let fd = cqe.result?;
        let fd = SharedFd::new(fd as i32);
        let socket = Socket { fd };
        Ok(socket)
    }

    fn stream_complete(self, cqe: CqeResult) -> Option<Self::Item> {
        // There is no None option returned by this stream completion method.
        // The poll_next_op will return that once the stream is polled an additional time.

        let fd = match cqe.result {
            Ok(result) => result,
            Err(e) => return Some(Err(e)),
        };
        let fd = SharedFd::new(fd as i32);
        let socket = Socket { fd };
        Some(Ok(socket))
    }
}
