use crate::buf::fixed::FixedBuf;
use crate::buf::{IoBuf, Slice};
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) struct WriteFixed {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    buf: Slice<FixedBuf>,
}

impl Op<WriteFixed> {
    pub(crate) fn write_fixed_at(
        fd: &SharedFd,
        buf: Slice<FixedBuf>,
        offset: u64,
    ) -> io::Result<Op<WriteFixed>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            WriteFixed {
                _fd: fd.clone(),
                buf,
            },
            |write_fixed| {
                // Get raw buffer info
                let ptr = write_fixed.buf.stable_ptr();
                let len = write_fixed.buf.bytes_init();
                let buf_index = write_fixed.buf.get_ref().buf_index();
                opcode::WriteFixed::new(types::Fd(fd.raw_fd()), ptr, len as _, buf_index)
                    .offset(offset as _)
                    .build()
            },
        )
    }

    pub(crate) async fn write_fixed(mut self) -> BufResult<usize, Slice<FixedBuf>> {
        use crate::future::poll_fn;

        poll_fn(move |cx| self.poll_write_fixed(cx)).await
    }

    pub(crate) fn poll_write_fixed(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<BufResult<usize, Slice<FixedBuf>>> {
        let complete = ready!(Pin::new(self).poll(cx));
        Poll::Ready((complete.result.map(|v| v as _), complete.data.buf))
    }
}
