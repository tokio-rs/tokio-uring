use crate::buf::{IoBufMut, Slice};
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use std::io;
use std::task::{Context, Poll};

pub(crate) struct Read<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// Owns the in-flight buffer.
    buf: Slice<T>,
}

impl<T: IoBufMut> Op<Read<T>> {
    pub(crate) fn read_at(
        fd: &SharedFd,
        buf: Slice<T>,
        offset: u64,
    ) -> io::Result<Op<Read<T>>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            Read {
                fd: fd.clone(),
                buf,
            },
            |read| {
                // Get raw buffer info
                let ptr = read.buf.stable_mut_ptr();
                let len = read.buf.bytes_total();
                opcode::Read::new(types::Fd(fd.raw_fd()), ptr, len as _)
                    .offset(offset as _)
                    .build()
            },
        )
    }

    pub(crate) async fn read(mut self) -> BufResult<usize, Slice<T>> {
        crate::future::poll_fn(move |cx| self.poll_read(cx)).await
    }

    pub(crate) fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<BufResult<usize, Slice<T>>> {
        use std::future::Future;
        use std::pin::Pin;

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
