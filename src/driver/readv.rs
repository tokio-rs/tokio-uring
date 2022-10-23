use crate::buf::IoBufMut;
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use crate::driver::op::{self, Completable};
use libc::iovec;
use std::io;

pub(crate) struct Readv<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// Reference to the in-flight buffer.
    pub(crate) bufs: Vec<T>,
    /// Parameter for `io_uring::op::readv`, referring `bufs`.
    iovs: Vec<iovec>,
}

impl<T: IoBufMut> Op<Readv<T>> {
    pub(crate) fn readv_at(
        fd: &SharedFd,
        mut bufs: Vec<T>,
        offset: u64,
    ) -> io::Result<Op<Readv<T>>> {
        use io_uring::{opcode, types};

        // Build `iovec` objects referring the provided `bufs` for `io_uring::opcode::Readv`.
        let iovs: Vec<iovec> = bufs
            .iter_mut()
            .map(|b| iovec {
                // Safety guaranteed by `IoBufMut`.
                iov_base: unsafe { b.stable_mut_ptr().add(b.bytes_init()) as *mut libc::c_void },
                iov_len: b.bytes_total() - b.bytes_init(),
            })
            .collect();

        Op::submit_with(
            Readv {
                fd: fd.clone(),
                bufs,
                iovs,
            },
            |read| {
                opcode::Readv::new(
                    types::Fd(fd.raw_fd()),
                    read.iovs.as_ptr(),
                    read.iovs.len() as u32,
                )
                .offset(offset as _)
                .build()
            },
        )
    }
}

impl<T> Completable for Readv<T>
where
    T: IoBufMut,
{
    type Output = BufResult<usize, Vec<T>>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let mut bufs = self.bufs;

        // If the operation was successful, advance the initialized cursor.
        if let Ok(n) = res {
            let mut count = n;
            for b in bufs.iter_mut() {
                let sz = std::cmp::min(count, b.bytes_total() - b.bytes_init());
                let pos = b.bytes_init() + sz;
                // Safety: the kernel returns bytes written, and we have ensured that `pos` is
                // valid for current buffer.
                unsafe { b.set_init(pos) };
                count -= sz;
                if count == 0 {
                    break;
                }
            }
            assert_eq!(count, 0);
        }

        (res, bufs)
    }
}
