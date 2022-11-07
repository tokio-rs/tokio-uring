use crate::driver::{self, Op, SharedFd};
use crate::fs::{File, OpenOptions};

use crate::driver::op::{self, Buildable, Completable};
use std::ffi::CString;
use std::io;
use std::path::Path;

/// Open a file
#[allow(dead_code)]
pub(crate) struct Open {
    pub(crate) path: CString,
    pub(crate) flags: libc::c_int,
    options: OpenOptions,
}

impl Op<Open> {
    /// Submit a request to open a file.
    pub(crate) fn open(path: &Path, options: &OpenOptions) -> io::Result<Op<Open>> {
        let path = driver::util::cstr(path)?;
        let flags = libc::O_CLOEXEC
            | options.access_mode()?
            | options.creation_mode()?
            | (options.custom_flags & !libc::O_ACCMODE);

        Open {
            path,
            flags,
            options: options.clone(),
        }
        .submit()
    }
}

impl Buildable for Open
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};

        // Get a reference to the memory. The string will be held by the
        // operation state and will not be accessed again until the operation
        // completes.
        let p_ref = self.path.as_c_str().as_ptr();

        opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), p_ref)
            .flags(self.flags)
            .mode(self.options.mode)
            .build()
    }
}

impl Completable for Open {
    type Output = io::Result<File>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        Ok(File::from_shared_fd(SharedFd::new(cqe.result? as _)))
    }
}
