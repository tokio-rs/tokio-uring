use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;

use super::util::cstr;

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
    pub(crate) fn make_dir(path: &Path, mode: rustix::fs::Mode) -> io::Result<Op<Mkdir>> {
        use rustix_uring::{opcode, types};

        let _path = cstr(path)?;

        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op(Mkdir { _path }, |mkdir| {
                    let p_ref = mkdir._path.as_c_str().as_ptr();

                    opcode::MkDirAt::new(types::Fd(libc::AT_FDCWD), p_ref)
                        .mode(mode)
                        .build()
                })
        })
    }
}

impl Completable for Mkdir {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
