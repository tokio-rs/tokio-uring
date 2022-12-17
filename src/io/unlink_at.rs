use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use std::ffi::CString;
use std::io;
use std::path::Path;

/// Unlink a path relative to the current working directory of the caller's process.
pub(crate) struct Unlink {
    pub(crate) path: CString,
}

impl Op<Unlink> {
    /// Submit a request to unlink a directory with provided flags.
    pub(crate) fn unlink_dir(path: &Path) -> io::Result<Op<Unlink>> {
        Self::unlink(path, libc::AT_REMOVEDIR)
    }

    /// Submit a request to unlink a file with provided flags.
    pub(crate) fn unlink_file(path: &Path) -> io::Result<Op<Unlink>> {
        Self::unlink(path, 0)
    }

    /// Submit a request to unlink a specified path with provided flags.
    pub(crate) fn unlink(path: &Path, flags: i32) -> io::Result<Op<Unlink>> {
        use io_uring::{opcode, types};

        let path = super::util::cstr(path)?;

        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op(Unlink { path }, |unlink| {
                    // Get a reference to the memory. The string will be held by the
                    // operation state and will not be accessed again until the operation
                    // completes.
                    let p_ref = unlink.path.as_c_str().as_ptr();
                    opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), p_ref)
                        .flags(flags)
                        .build()
                })
        })
    }
}

impl Completable for Unlink {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
