use crate::buf::IoBufMut;
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use crate::driver::op::{self, Buildable, Completable};
use std::io;

pub(crate) struct Read<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// Reference to the in-flight buffer.
    pub(crate) buf: T,

    offset: u64,
}

impl<T: IoBufMut> Op<Read<T>> {
    pub(crate) fn read_at(fd: &SharedFd, buf: T, offset: u64) -> io::Result<Op<Read<T>>> {
        Read {
            fd: fd.clone(),
            buf,
            offset,
        }
        .submit()
    }
}

impl<T: IoBufMut> Buildable for Read<T>
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = self.buf.stable_mut_ptr();
        let len = self.buf.bytes_total();
        opcode::Read::new(types::Fd(self.fd.raw_fd()), ptr, len as _)
            .offset(self.offset as _)
            .build()
    }
}

impl<T> Completable for Read<T>
where
    T: IoBufMut,
{
    type Output = BufResult<usize, T>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
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
