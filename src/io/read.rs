use crate::buf::BoundedBufMut;
use crate::io::SharedFd;
use crate::BufResult;

use crate::io::shared_fd::sealed::CommonFd;
use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use std::io;

pub(crate) struct Read<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// Reference to the in-flight buffer.
    pub(crate) buf: T,
}

impl<T: BoundedBufMut> Op<Read<T>> {
    pub(crate) fn read_at(fd: &SharedFd, buf: T, offset: u64) -> io::Result<Op<Read<T>>> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                Read {
                    fd: fd.clone(),
                    buf,
                },
                |read| {
                    // Get raw buffer info
                    let ptr = read.buf.stable_mut_ptr();
                    let len = read.buf.bytes_total();
                    match read.fd.common_fd() {
                        CommonFd::Raw(raw) => {
                            let fd = types::Fd(raw);
                            opcode::Read::new(fd, ptr, len as _)
                                .offset(offset as _)
                                .build()
                        }
                        CommonFd::Fixed(fixed) => {
                            let fd = types::Fixed(fixed);
                            opcode::Read::new(fd, ptr, len as _)
                                .offset(offset as _)
                                .build()
                        }
                    }
                },
            )
        })
    }
}

impl<T> Completable for Read<T>
where
    T: BoundedBufMut,
{
    type Output = BufResult<usize, T>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
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
