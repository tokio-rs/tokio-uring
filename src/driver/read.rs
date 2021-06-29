use crate::buf::IoBufMut;
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use std::io;
use std::task::{Context, Poll};

pub(crate) struct Read<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    /// Reference to the in-flight buffer.
    pub(crate) buf: Option<T>,
}

impl<T: IoBufMut> Op<Read<T>> {
    pub(crate) fn read_at(fd: &SharedFd, mut buf: T, offset: u64) -> io::Result<Op<Read<T>>> {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.stable_mut_ptr();
        let len = buf.bytes_total();

        Op::submit_with(
            Read {
                _fd: fd.clone(),
                buf: Some(buf),
            },
            || {
                opcode::Read::new(types::Fd(fd.raw_fd()), ptr, len as _)
                    .offset(offset as _)
                    .build()
            },
        )
    }

    pub(crate) async fn read(mut self) -> BufResult<usize, T> {
        crate::future::poll_fn(move |cx| self.poll_read(cx)).await
    }

    pub(crate) fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<BufResult<usize, T>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));

        // Convert the operation result to `usize`
        let res = complete.result.map(|v| v as usize);
        // Recover the buffer
        let mut buf = complete.data.buf.unwrap();

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
