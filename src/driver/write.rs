use crate::BufResult;
use crate::buf;
use crate::driver::Op;

use futures::ready;
use std::io;
use std::os::unix::io::RawFd;
use std::task::{Context, Poll};

pub(crate) struct Write {
    pub(crate) buf: buf::Slice,
}

impl Op<Write> {
    pub(crate) fn write_at(fd: RawFd, buf: buf::Slice, offset: u64) -> io::Result<Op<Write>> {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.as_ptr();
        let len = buf.len();

        Op::submit_with(Write {
            buf,
        }, |_| {
            opcode::Write::new(types::Fd(fd), ptr, len as _)
                .offset(offset as _)
                .build()
        })
    }

    pub(crate) async fn write(mut self) -> BufResult<usize> {
        use futures::future::poll_fn;

        poll_fn(move |cx| self.poll_write(cx)).await
    }

    pub(crate) fn poll_write(&mut self, cx: &mut Context<'_>) -> Poll<BufResult<usize>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));
        Poll::Ready((complete.result.map(|v| v as _), complete.state.buf))
    }
}