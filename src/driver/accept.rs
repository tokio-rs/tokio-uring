use crate::driver::Op;

use std::io;
use std::os::unix::io::RawFd;
use std::ptr;

pub(crate) struct Accept;

impl Op<Accept> {
    pub(crate) fn accept(fd: RawFd) -> io::Result<Op<Accept>> {
        use io_uring::{opcode, types};

        Op::submit_with(Accept, |_| {
            opcode::Accept::new(types::Fd(fd), ptr::null_mut(), ptr::null_mut())
                .flags(libc::SOCK_CLOEXEC)
                .build()
        })
    }
}

