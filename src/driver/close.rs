use crate::driver::Op;

use crate::driver::op::Completable;
use std::io;
use std::os::unix::io::RawFd;

pub(crate) struct Close {
    fd: RawFd,
}

impl Op<Close> {
    pub(crate) fn close(fd: RawFd) -> io::Result<Op<Close>> {
        use io_uring::{opcode, types};

        Op::try_submit_with(Close { fd }, |close| {
            opcode::Close::new(types::Fd(close.fd)).build()
        })
    }
}

impl Completable for Close {
    type Output = io::Result<()>;

    fn complete(self, result: io::Result<u32>, _flags: u32) -> Self::Output {
        let _ = result?;

        Ok(())
    }
}
