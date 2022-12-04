use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
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
}

impl Op<RenameAt> {
    /// Submit a request to rename a specified path to a new name with
    /// the provided flags.
    pub(crate) fn rename_at(from: &Path, to: &Path, flags: u32) -> io::Result<Op<RenameAt>> {
        use io_uring::{opcode, types};

        let from = super::util::cstr(from)?;
        let to = super::util::cstr(to)?;

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                RenameAt { from, to },
                |rename| {
                    // Get a reference to the memory. The string will be held by the
                    // operation state and will not be accessed again until the operation
                    // completes.
                    let from_ref = rename.from.as_c_str().as_ptr();
                    let to_ref = rename.to.as_c_str().as_ptr();
                    opcode::RenameAt::new(
                        types::Fd(libc::AT_FDCWD),
                        from_ref,
                        types::Fd(libc::AT_FDCWD),
                        to_ref,
                    )
                    .flags(flags)
                    .build()
                },
            )
        })
    }
}

impl Completable for RenameAt {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
