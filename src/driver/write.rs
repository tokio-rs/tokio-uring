use crate::buf::IoBuf;
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use futures::ready;
use std::io;
use std::task::{Context, Poll};

pub(crate) struct Write<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    pub(crate) buf: T,
}

impl<T: IoBuf> Op<Write<T>> {
    pub(crate) fn write_at(fd: &SharedFd, buf: T, offset: u64) -> io::Result<Op<Write<T>>> {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.stable_ptr();
        let len = buf.bytes_init();

        Op::submit_with(
            Write {
                _fd: fd.clone(),
                buf,
            },
            || {
                opcode::Write::new(types::Fd(fd.raw_fd()), ptr, len as _)
                    .offset(offset as _)
                    .build()
            },
        )
    }

    pub(crate) async fn write(mut self) -> BufResult<usize, T> {
        use futures::future::poll_fn;

        poll_fn(move |cx| self.poll_write(cx)).await
    }

    pub(crate) fn poll_write(&mut self, cx: &mut Context<'_>) -> Poll<BufResult<usize, T>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));
        Poll::Ready((complete.result.map(|v| v as _), complete.data.buf))
    }
}
