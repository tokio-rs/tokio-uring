use crate::buf::BoundedBufMut;
use crate::io::SharedFd;
use crate::BufResult;

use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use std::io;

pub(crate) struct Recv<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// Reference to the in-flight buffer.
    pub(crate) buf: T,
}

impl<T: BoundedBufMut> Op<Recv<T>> {
    // TODO remove allow
    #[allow(dead_code)]
    pub(crate) fn recv(fd: &SharedFd, buf: T, flags: Option<i32>) -> io::Result<Op<Recv<T>>> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                Recv {
                    fd: fd.clone(),
                    buf,
                },
                |s| {
                    // Get raw buffer info
                    let ptr = s.buf.stable_mut_ptr();
                    let len = s.buf.bytes_total();
                    opcode::Recv::new(types::Fd(fd.raw_fd()), ptr, len as _)
                        .flags(flags.unwrap_or(0))
                        .build()
                },
            )
        })
    }
}

impl<T> Completable for Recv<T>
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
