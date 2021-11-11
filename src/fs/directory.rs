use crate::driver::Op;

use std::fs::ReadDir;
use std::io;
use std::path::Path;

/// Returns an iteratory over the entries in a directory.
pub async fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<ReadDir> {
    todo!();
}

/// Removes an empty directory.
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    // TODO yikes
    let op = Op::unlink(path.as_ref()).unwrap();
    let completion = op.await;
    completion.result?;

    Ok(())
}

pub async fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    todo!();
}
