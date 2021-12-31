use crate::driver::{self, Op};

use std::ffi::CString;
use std::io;
use std::path::Path;

/// Create a directory at path relative to the current working directory
/// of the caller's process.
pub(crate) struct Mkdir {
    pub(crate) _path: CString,
}

impl Op<Mkdir> {
    /// Submit a request to create a directory
    pub(crate) fn make_dir(path: &Path) -> io::Result<Op<Mkdir>> {
        use io_uring::{opcode, types};

        let _path = driver::util::cstr(path)?;

        // Get a reference to the memory. The string will be held by the
        // operation state and will not be accessed again until the operation
        // completes.
        let p_ref = _path.as_c_str().as_ptr();

        Op::submit_with(Mkdir { _path }, || {
            opcode::MkDirAt::new(types::Fd(libc::AT_FDCWD), p_ref)
                .build()
        })
    }
}
