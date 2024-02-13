use crate::runtime::driver::op::{Completable, CqeResult, MultiCQEFuture, Op, Updateable};
use crate::runtime::CONTEXT;
use crate::{buf::BoundedBuf, io::SharedFd, Result};
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

impl<T: BoundedBuf> Op<SendZc<T>, MultiCQEFuture> {
    pub(crate) fn send_zc(fd: &SharedFd, buf: T) -> io::Result<Self> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
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
        })
    }
}

impl<T> Completable for SendZc<T> {
    type Output = Result<usize, T>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| self.bytes + v as usize);
        // Recover the buffer
        let buf = self.buf;
        match res {
            Ok(n) => Ok((n, buf)),
            Err(e) => Err(crate::Error(e, buf)),
        }
    }
}

impl<T> Updateable for SendZc<T> {
    fn update(&mut self, cqe: CqeResult) {
        // uring send_zc promises there will be no error on CQE's marked more
        self.bytes += *cqe.result.as_ref().unwrap() as usize;
    }
}
