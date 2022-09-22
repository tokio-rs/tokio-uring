use crate::buf::IoBufMut;
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use crate::driver::op::Completable;
use std::io;
use std::task::{Context, Poll};

pub(crate) struct Read<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// Reference to the in-flight buffer.
    pub(crate) buf: T,
}

impl<T: IoBufMut> Op<Read<T>> {
    pub(crate) fn read_at(fd: &SharedFd, buf: T, offset: u64) -> io::Result<Op<Read<T>>> {
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

    pub(crate) async fn read(mut self) -> BufResult<usize, T> {
        crate::future::poll_fn(move |cx| self.poll_read(cx)).await
    }

    pub(crate) fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<BufResult<usize, T>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));

        Poll::Ready(complete)
    }
}

impl<T> Completable for Read<T>
where
    T: IoBufMut,
{
    type Output = BufResult<usize, T>;

    fn complete(self, result: io::Result<u32>, _flags: u32) -> Self::Output {
        // Convert the operation result to `usize`
        let res = result.map(|v| v as usize);
        // Recover the buffer
        let mut buf = self.buf;

        // If the operation was successful, advance the initialized cursor.
        if let Ok(n) = res {
            // Safety: the kernel wrote `n` bytes to the buffer.
            unsafe {
                buf.set_init(n);
            }
        }

        (res, buf)
    }
}
