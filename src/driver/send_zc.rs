use crate::driver::op::{self, Completable, Updateable};
use crate::{
    buf::IoBuf,
    driver::{Op, SharedFd},
    BufResult,
};
use std::io;

pub(crate) struct SendZc<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    pub(crate) buf: T,

    /// Hold the number of transmitted bytes
    bytes: usize,
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

                opcode::SendZc::new(types::Fd(fd.raw_fd()), ptr, len as _).build()
            },
        )
    }
}

impl<T> Completable for SendZc<T>
where
    T: IoBuf,
{
    type Output = BufResult<usize, T>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| self.bytes + v as usize);
        // Recover the buffer
        let buf = self.buf;
        (res, buf)
    }
}

impl<T> Updateable for SendZc<T>
where
    T: IoBuf,
{
    fn update(&mut self, cqe: op::CqeResult) {
        // uring send_zc promises there will be no error on CQE's marked more
        self.bytes += *cqe.result.as_ref().unwrap() as usize;
    }
}
