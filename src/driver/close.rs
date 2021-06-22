use crate::driver::Op;

use std::io;
use std::os::unix::io::RawFd;

pub(crate) struct Close;

impl Op<Close> {
    pub(crate) fn close(fd: RawFd) -> io::Result<Op<Close>> {
        use io_uring::{opcode, types};

        Op::submit_with(Close, || opcode::Close::new(types::Fd(fd)).build())
    }
}
