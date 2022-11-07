use crate::driver::op::{self, Buildable, Completable};
use crate::{
    buf::IoBuf,
    driver::{Op, SharedFd},
    BufResult,
};
use libc::iovec;
use std::io;

pub(crate) struct Writev<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    pub(crate) bufs: Vec<T>,

    /// Parameter for `io_uring::op::readv`, referring `bufs`.
    iovs: Vec<iovec>,

    offset: u64,
}

impl<T: IoBuf> Op<Writev<T>> {
    pub(crate) fn writev_at(
        fd: &SharedFd,
        mut bufs: Vec<T>,
        offset: u64,
    ) -> io::Result<Op<Writev<T>>> {
        // Build `iovec` objects referring the provided `bufs` for `io_uring::opcode::Readv`.
        let iovs: Vec<iovec> = bufs
            .iter_mut()
            .map(|b| iovec {
                iov_base: b.stable_ptr() as *mut libc::c_void,
                iov_len: b.bytes_init(),
            })
            .collect();

        Writev {
            fd: fd.clone(),
            bufs,
            iovs,
            offset,
        }
        .submit()
    }
}

impl<T: IoBuf> Buildable for Writev<T>
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};

        opcode::Writev::new(
            types::Fd(self.fd.raw_fd()),
            self.iovs.as_ptr(),
            self.iovs.len() as u32,
        )
        .offset(self.offset as _)
        .build()
    }
}

impl<T> Completable for Writev<T>
where
    T: IoBuf,
{
    type Output = BufResult<usize, Vec<T>>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let buf = self.bufs;

        (res, buf)
    }
}
