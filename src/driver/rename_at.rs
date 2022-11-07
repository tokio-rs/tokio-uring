use crate::driver::{self, Op};

use crate::driver::op::{self, Buildable, Completable};
use std::ffi::CString;
use std::io;
use std::path::Path;

/// Renames a file, moving it between directories if required.
///
/// The given paths are interpreted relative to the current working directory
/// of the calling process.
pub(crate) struct RenameAt {
    pub(crate) from: CString,
    pub(crate) to: CString,
    flags: u32,
}

impl Op<RenameAt> {
    /// Submit a request to rename a specified path to a new name with
    /// the provided flags.
    pub(crate) fn rename_at(from: &Path, to: &Path, flags: u32) -> io::Result<Op<RenameAt>> {
        let from = driver::util::cstr(from)?;
        let to = driver::util::cstr(to)?;

        RenameAt { from, to, flags }.submit()
    }
}

impl Buildable for RenameAt
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};

        // Get a reference to the memory. The string will be held by the
        // operation state and will not be accessed again until the operation
        // completes.
        let from_ref = self.from.as_c_str().as_ptr();
        let to_ref = self.to.as_c_str().as_ptr();
        opcode::RenameAt::new(
            types::Fd(libc::AT_FDCWD),
            from_ref,
            types::Fd(libc::AT_FDCWD),
            to_ref,
        )
        .flags(self.flags)
        .build()
    }
}

impl Completable for RenameAt {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
