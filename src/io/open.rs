use crate::fs::{File, OpenOptions};
use crate::io::SharedFd;

use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use std::ffi::CString;
use std::io;
use std::os::unix::io::RawFd;
use std::path::Path;

/// Open a file
#[allow(dead_code)]
pub(crate) struct Open {
    path: CString,
    flags: libc::c_int,
    fixed_table_auto_select: bool,
}

impl Op<Open> {
    /// Submit a request to open a file.
    pub(crate) fn open(path: &Path, options: &OpenOptions) -> io::Result<Op<Open>> {
        use io_uring::{opcode, types};
        let path = super::util::cstr(path)?;
        let flags = libc::O_CLOEXEC
            | options.access_mode()?
            | options.creation_mode()?
            | (options.custom_flags & !libc::O_ACCMODE);

        let (file_index, fixed_table_auto_select) = if options.fixed_table_slot {
            (Some(types::DestinationSlot::auto_target()), true)
        } else {
            (None, false)
        };

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                Open {
                    path,
                    flags,
                    fixed_table_auto_select,
                },
                |open| {
                    // Get a reference to the memory. The string will be held by the
                    // operation state and will not be accessed again until the operation
                    // completes.
                    let p_ref = open.path.as_c_str().as_ptr();

                    opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), p_ref)
                        .flags(flags)
                        .mode(options.mode)
                        .file_index(file_index)
                        .build()
                },
            )
        })
    }
}

impl Completable for Open {
    type Output = io::Result<File>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        let result = cqe.result?;
        let shared_fd = if self.fixed_table_auto_select {
            SharedFd::new_fixed(result)
        } else {
            SharedFd::new(result as RawFd)
        };

        Ok(File::from_shared_fd(shared_fd))
    }
}
