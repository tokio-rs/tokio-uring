use crate::BufResult;
use crate::driver::{Op, SharedFd};
use crate::buf::{IoBuf, Slice};

use std::io;
use std::path::Path;

pub struct File {
    /// Open file descriptor
    fd: SharedFd,
}

impl File {
    /// Attempts to open a file in read-only mode.
    pub async fn open<P: AsRef<Path>>(path: P) -> io::Result<File> {
        let op = Op::open(path.as_ref())?;

        // Await the completion of the event
        let completion = op.await;

        // The file is open
        Ok(File {
            fd: SharedFd::new(completion.result? as _),
        })
    }

    // Positional reads FTW
    pub async fn read_at(&self, buf: impl Into<Slice>, pos: u64) -> BufResult<usize> {
        // Submit the read operation
        let op = Op::read_at(self.fd.fd(), buf.into(), pos).unwrap();
        op.read().await
    }

    /// What is a better name?
    pub async fn read_at2(&self, pos: u64, len: usize) -> io::Result<IoBuf> {
        let op = Op::read_at2(self.fd.fd(), pos, len).unwrap();
        op.read2().await
    }

    /// Close the file
    pub async fn close(self) -> io::Result<()> {
        self.fd.close().await;
        Ok(())
    }
}
