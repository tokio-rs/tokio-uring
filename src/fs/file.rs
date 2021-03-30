use crate::BufResult;
use crate::driver::Op;
use crate::buf::{IoBuf, Slice};

use std::io;
use std::os::unix::io::RawFd;
use std::path::Path;

pub struct File {
    /// Open file descriptor
    fd: RawFd,
}

impl File {
    /// Attempts to open a file in read-only mode.
    pub async fn open<P: AsRef<Path>>(path: P) -> io::Result<File> {
        let op = Op::open(path.as_ref())?;

        // Await the completion of the event
        let completion = op.await;

        // The file is open
        Ok(File {
            fd: completion.result? as _,
        })
    }

    // Positional reads FTW
    pub async fn read_at(&self, buf: impl Into<Slice>, pos: u64) -> BufResult<usize> {
        // Submit the read operation
        let op = Op::read_at(self.fd, buf.into(), pos).unwrap();
        op.read().await
    }

    /// What is a better name?
    pub async fn read_at2(&self, pos: u64, len: usize) -> io::Result<IoBuf> {
        let op = Op::read_at2(self.fd, pos, len).unwrap();
        op.read2().await
    }

    /// Close the file
    pub async fn close(mut self) -> io::Result<()> {
        let op = Op::close(self.fd)?;
        let completion = op.await;

        self.fd = 0;
        drop(self);

        completion.result.map(|_| ())
    }
}

impl Drop for File {
    fn drop(&mut self) {
        use std::os::unix::io::FromRawFd;

        if self.fd == 0 { return }

        // TODO: warn? Do something better?
        let _ = unsafe { std::fs::File::from_raw_fd(self.fd) };
    }
}