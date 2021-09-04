use crate::buf::fixed::FixedBuf;
use crate::buf::{IoBuf, IoBufMut, Slice};
use crate::driver::op::{self, Completable};
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use std::io;

pub(crate) struct ReadFixed {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// The in-flight buffer.
    buf: Slice<FixedBuf>,
}

impl Op<ReadFixed> {
    pub(crate) fn read_fixed_at(
        fd: &SharedFd,
        buf: Slice<FixedBuf>,
        offset: u64,
    ) -> io::Result<Op<ReadFixed>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            ReadFixed {
                fd: fd.clone(),
                buf,
            },
            |read_fixed| {
                // Get raw buffer info
                let ptr = read_fixed.buf.stable_mut_ptr();
                let len = read_fixed.buf.bytes_total();
                let buf_index = read_fixed.buf.get_ref().buf_index();
                opcode::ReadFixed::new(types::Fd(fd.raw_fd()), ptr, len as _, buf_index)
                    .offset(offset as _)
                    .build()
            },
        )
    }
}

impl Completable for ReadFixed {
    type Output = BufResult<usize, Slice<FixedBuf>>;

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
