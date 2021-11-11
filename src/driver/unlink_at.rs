use crate::driver::{self, Op};

use std::ffi::CString;
use std::io;
use std::path::Path;

/// Unlink a file
pub(crate) struct Unlink {
    pub(crate) _path: CString,
}

impl Op<Unlink> {
    /// Submit a request to unlink a file.
    pub(crate) fn unlink(path: &Path) -> io::Result<Op<Unlink>> {
        use io_uring::{opcode, types};

        let path = driver::util::cstr(path)?;

        // Get a reference to the memory. The string will be held by the
        // operation state and will not be accessed again until the operation
        // completes.
        let p_ref = path.as_c_str().as_ptr();

        let flags = libc::AT_REMOVEDIR;

        Op::submit_with(Unlink { _path: path }, || {
            opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), p_ref)
                .flags(flags)
                .build()
        })
    }
}
