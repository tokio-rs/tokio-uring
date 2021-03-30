use crate::driver::{self, Op};

use std::ffi::CString;
use std::io;
use std::path::Path;

/// Open a file
pub(crate) struct Open {
    pub(crate) path: CString,
}

impl Op<Open> {
    /// Submit a request to open a file.
    pub(crate) fn open(path: &Path) -> io::Result<Op<Open>> {
        use io_uring::{opcode, types};

        let path = driver::util::cstr(path)?;

        Op::submit_with(Open {
            path,
        }, |state| {
            // Get a reference to the memory. The string is held by the
            // operation state and will not be accessed again until the
            // operation completes.
            let p_ref = state.path.as_c_str().as_ptr();

            // Set flag & mode
            let flags = libc::O_CLOEXEC | libc::O_RDONLY;

            opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), p_ref)
                .flags(flags)
                .mode(0o666)
                .build()
        })
    }
}