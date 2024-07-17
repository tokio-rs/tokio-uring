use crate::runtime::driver::op::Op;
use std::io;
use std::path::Path;

/// Creates a new symbolic link on the filesystem.
/// The dst path will be a symbolic link pointing to the src path.
/// This is an async version of std::os::unix::fs::symlink.
pub async fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<()> {
    Op::symlink(src, dst)?.await
}
