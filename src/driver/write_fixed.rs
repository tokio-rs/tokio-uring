use crate::buf::fixed::FixedBuf;
use crate::buf::{IoBuf, Slice};
use crate::driver::op::{self, Completable};
use crate::driver::{Op, SharedFd};
use crate::BufResult;

use std::io;

pub(crate) struct WriteFixed {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    buf: Slice<FixedBuf>,
}

impl Op<WriteFixed> {
    pub(crate) fn write_fixed_at(
        fd: &SharedFd,
        buf: Slice<FixedBuf>,
        offset: u64,
    ) -> io::Result<Op<WriteFixed>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            WriteFixed {
                fd: fd.clone(),
                buf,
            },
            |write_fixed| {
                // Get raw buffer info
                let ptr = write_fixed.buf.stable_ptr();
                let len = write_fixed.buf.bytes_init();
                let buf_index = write_fixed.buf.get_ref().buf_index();
                opcode::WriteFixed::new(types::Fd(fd.raw_fd()), ptr, len as _, buf_index)
                    .offset(offset as _)
                    .build()
            },
        )
    }
}

impl Completable for WriteFixed {
    type Output = BufResult<usize, Slice<FixedBuf>>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let buf = self.buf;

        (res, buf)
    }
}
