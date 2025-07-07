use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;

use super::util::cstr;

use std::ffi::CString;
use std::io;
use std::path::Path;

pub(crate) struct Symlink {
    pub(crate) _from: CString,
    pub(crate) _to: CString,
}

impl Op<Symlink> {
    pub(crate) fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(
        from: P,
        to: Q,
    ) -> io::Result<Op<Symlink>> {
        use rustix_uring::{opcode, types};

        let _from = cstr(from.as_ref())?;
        let _to = cstr(to.as_ref())?;

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                Symlink { _from, _to },
                |symlink| {
                    let from_ref = symlink._from.as_c_str().as_ptr();
                    let to_ref = symlink._to.as_c_str().as_ptr();

                    opcode::SymlinkAt::new(types::Fd(libc::AT_FDCWD), from_ref, to_ref).build()
                },
            )
        })
    }
}

impl Completable for Symlink {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}
