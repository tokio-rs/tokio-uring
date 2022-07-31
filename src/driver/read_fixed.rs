use crate::buf::fixed::FixedBuf;
use crate::buf::{IoBuf, IoBufMut, Slice};
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) struct ReadFixed {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    /// The in-flight buffer.
    buf: Slice<FixedBuf>,
}

impl Op<ReadFixed> {
    pub(crate) fn read_fixed_at(
        fd: &SharedFd,
        buf: Slice<FixedBuf>,
        offset: u64,
    ) -> io::Result<Op<ReadFixed>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            ReadFixed {
                _fd: fd.clone(),
                buf,
            },
            |read_fixed| {
                // Get raw buffer info
                let ptr = read_fixed.buf.stable_mut_ptr();
                let len = read_fixed.buf.bytes_total();
                let buf_index = read_fixed.buf.get_ref().buf_index();
                opcode::ReadFixed::new(types::Fd(fd.raw_fd()), ptr, len as _, buf_index)
                    .offset(offset as _)
                    .build()
            },
        )
    }

    pub(crate) async fn read_fixed(mut self) -> BufResult<usize, Slice<FixedBuf>> {
        crate::future::poll_fn(move |cx| self.poll_read_fixed(cx)).await
    }

    pub(crate) fn poll_read_fixed(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<BufResult<usize, Slice<FixedBuf>>> {
        let complete = ready!(Pin::new(self).poll(cx));

        // Convert the operation result to `usize`
        let res = complete.result.map(|v| v as usize);
        // Recover the buffer
        let mut buf = complete.data.buf;

        // If the operation was successful, advance the initialized cursor.
        if let Ok(n) = res {
            // Safety: the kernel wrote `n` bytes to the buffer.
            unsafe {
                buf.set_init(n);
            }
        }

        Poll::Ready((res, buf))
    }
}
