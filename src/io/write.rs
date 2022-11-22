use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::{buf::BoundedBuf, io::SharedFd, BufResult};
use std::io;

pub(crate) struct Write<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    buf: T,
}

impl<T: BoundedBuf> Op<Write<T>> {
    pub(crate) fn write_at(fd: &SharedFd, buf: T, offset: u64) -> io::Result<Op<Write<T>>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            Write {
                fd: fd.clone(),
                buf,
            },
            |write| {
                // Get raw buffer info
                let ptr = write.buf.stable_ptr();
                let len = write.buf.bytes_init();

                opcode::Write::new(types::Fd(fd.raw_fd()), ptr, len as _)
                    .offset(offset as _)
                    .build()
            },
        )
    }
}

impl<T> Completable for Write<T> {
    type Output = BufResult<usize, T>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let buf = self.buf;

        (res, buf)
    }
}
