use crate::driver::{self, Op};
use crate::fs::OpenOptions;

use std::ffi::CString;
use std::io;
use std::path::Path;

/// Open a file
pub(crate) struct Open {
    pub(crate) _path: CString,
}

impl Op<Open> {
    /// Submit a request to open a file.
    pub(crate) fn open(path: &Path, options: &OpenOptions, direct: bool) -> io::Result<Op<Open>> {
        use io_uring::{opcode, types};

        let path = driver::util::cstr(path)?;
        // Get a reference to the memory. The string will be held by the
        // operation state and will not be accessed again until the operation
        // completes.
        let p_ref = path.as_c_str().as_ptr();

        let mut flags = libc::O_CLOEXEC | options.access_mode()? | options.creation_mode()?;

        if direct {
            flags |= libc::O_DIRECT;
        }

        Op::submit_with(Open { _path: path }, || {
            opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), p_ref)
                .flags(flags)
                .mode(options.mode)
                .build()
        })
    }
}
