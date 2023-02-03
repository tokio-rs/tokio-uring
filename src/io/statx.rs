use std::{ffi::CStr, io};

use io_uring::{opcode, types};

use crate::runtime::{
    driver::op::{Completable, CqeResult, Op},
    CONTEXT,
};

use super::SharedFd;

pub(crate) struct Statx {
    fd: SharedFd,
    statx: Box<libc::statx>,
}

impl Op<Statx> {
    pub(crate) fn statx(fd: &SharedFd, flags: i32, mask: u32) -> io::Result<Op<Statx>> {
        CONTEXT.with(|x| {
            let empty_path = CStr::from_bytes_with_nul(b"\0").unwrap();
            x.handle().expect("not in a runtime context").submit_op(
                Statx {
                    fd: fd.clone(),
                    statx: Box::new(unsafe { std::mem::zeroed() }),
                },
                |statx| {
                    opcode::Statx::new(
                        types::Fd(statx.fd.raw_fd()),
                        empty_path.as_ptr(),
                        &mut *statx.statx as *mut libc::statx as *mut types::statx,
                    )
                    .flags(flags)
                    .mask(mask)
                    .build()
                },
            )
        })
    }
}

impl Completable for Statx {
    type Output = io::Result<libc::statx>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result?;
        Ok(*self.statx)
    }
}
