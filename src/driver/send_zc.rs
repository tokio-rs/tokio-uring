use crate::driver::op::{Completable, Updateable};
use crate::{
    buf::IoBuf,
    driver::{Op, SharedFd},
    BufResult,
};
use std::{
    io,
    task::{Context, Poll},
};

pub(crate) struct SendZc<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    pub(crate) buf: T,

    /// Hold the number of transmitted bytes
    bytes: usize
}

impl<T: IoBuf> Op<SendZc<T>> {
    pub(crate) fn send_zc(fd: &SharedFd, buf: T) -> io::Result<Op<SendZc<T>>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            SendZc {
                fd: fd.clone(),
                buf,
                bytes: 0,
            },
            |send| {
                // Get raw buffer info
                let ptr = send.buf.stable_ptr();
                let len = send.buf.bytes_init();

                opcode::SendZc::new(types::Fd(fd.raw_fd()), ptr, len as _)
                    .build()
            },
        )
    }

    pub(crate) async fn send(mut self) -> BufResult<usize, T> {
        use crate::future::poll_fn;

        poll_fn(move |cx| self.poll_send(cx)).await
    }

    pub(crate) fn poll_send(&mut self, cx: &mut Context<'_>) -> Poll<BufResult<usize, T>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));
        Poll::Ready(complete)
    }
}

impl<T> Completable for SendZc<T>
where
    T: IoBuf,
{
    type Output = BufResult<usize, T>;

    fn complete(self, result: io::Result<u32>, _flags: u32) -> Self::Output {
        // Convert the operation result to `usize`
        let res = result.map(|v| self.bytes + v as usize);
        // Recover the buffer
        let buf = self.buf;
        (res, buf)
    }
}

impl<T> Updateable for SendZc<T>
where
    T: IoBuf,
{
    fn update(&mut self, cqe :&(io::Result<u32>, u32)) {
        // uring send_zc promises there will be no error on CQE's marked more
        self.bytes += *cqe.0.as_ref().unwrap() as usize;
    }
}