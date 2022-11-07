use crate::driver::op::{self, Buildable, Completable};
use crate::{
    buf::IoBuf,
    driver::{Op, SharedFd},
    BufResult,
};
use std::io;

pub(crate) struct Write<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    pub(crate) buf: T,

    offset: u64,
}

impl<T: IoBuf> Op<Write<T>> {
    pub(crate) fn write_at(fd: &SharedFd, buf: T, offset: u64) -> io::Result<Op<Write<T>>> {
        Write {
            fd: fd.clone(),
            buf,
            offset,
        }
        .submit()
    }
}

impl<T: IoBuf> op::Buildable for Write<T>
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = self.buf.stable_ptr();
        let len = self.buf.bytes_init();

        opcode::Write::new(types::Fd(self.fd.raw_fd()), ptr, len as _)
            .offset(self.offset as _)
            .build()
    }
}

impl<T> Completable for Write<T>
where
    T: IoBuf,
{
    type Output = BufResult<usize, T>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let buf = self.buf;

        (res, buf)
    }
}
