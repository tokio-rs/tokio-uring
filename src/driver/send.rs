use std::io;
use std::task::{Context, Poll};

use crate::buf::IoBuf;
use crate::BufResult;
use crate::driver::{Op, SharedFd};

pub(crate) struct Send<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    pub(crate) buf: T,
}

impl<T: IoBuf> Op<Send<T>> {
    pub(crate) fn send(fd: &SharedFd, buf: T) -> io::Result<Op<Send<T>>> {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.stable_ptr();
        let len = buf.bytes_init();

        Op::submit_with(
            Send {
                _fd: fd.clone(),
                buf,
            },
            || {
                opcode::Send::new(types::Fd(fd.raw_fd()), ptr, len as _)
                    .build()
            },
        )
    }

    pub(crate) async fn complete_send(mut self) -> BufResult<usize, T> {
        use crate::future::poll_fn;

        poll_fn(move |cx| self.poll_send(cx)).await
    }

    fn poll_send(&mut self, cx: &mut Context<'_>) -> Poll<BufResult<usize, T>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));
        Poll::Ready((complete.result.map(|v| v as _), complete.data.buf))
    }
}
