use crate::driver::op::{self, Completable};
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
}

impl<T: IoBuf> Op<Writev<T>> {
    pub(crate) fn writev_at(
        fd: &SharedFd,
        mut bufs: Vec<T>,
        offset: u64,
    ) -> io::Result<Op<Writev<T>>> {
        use io_uring::{opcode, types};

        // Build `iovec` objects referring the provided `bufs` for `io_uring::opcode::Readv`.
        let iovs: Vec<iovec> = bufs
            .iter_mut()
            .map(|b| iovec {
                iov_base: b.stable_ptr() as *mut libc::c_void,
                iov_len: b.bytes_init(),
            })
            .collect();

        Op::submit_with(
            Writev {
                fd: fd.clone(),
                bufs,
                iovs,
            },
            |write| {
                opcode::Writev::new(
                    types::Fd(fd.raw_fd()),
                    write.iovs.as_ptr(),
                    write.iovs.len() as u32,
                )
                .offset(offset as _)
                .build()
            },
        )
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
