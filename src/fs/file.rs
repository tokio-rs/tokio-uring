use crate::buf::{IoBuf, IoBufMut};
use crate::driver::{Op, SharedFd};
use crate::fs::OpenOptions;

use std::io;
use std::path::Path;

pub struct File {
    /// Open file descriptor
    fd: SharedFd,
}

impl File {
    /// Attempts to open a file in read-only mode.
    pub async fn open(path: impl AsRef<Path>) -> io::Result<File> {
        OpenOptions::new().read(true).open(path).await
    }

    /// Opens a file in write-only mode.
    pub async fn create(path: impl AsRef<Path>) -> io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .await
    }

    pub(crate) fn from_shared_fd(fd: SharedFd) -> File {
        File { fd }
    }

    // Positional reads FTW
    pub async fn read_at<T: IoBufMut>(&self, buf: T, pos: u64) -> crate::BufMutResult<usize, T> {
        // Submit the read operation
        let op = Op::read_at(self.fd.fd(), buf, pos).unwrap();
        op.read().await
    }

    pub async fn write_at<T: IoBuf>(&self, buf: T, pos: u64) -> crate::BufResult<usize, T> {
        let op = Op::write_at(self.fd.fd(), buf, pos).unwrap();
        op.write().await
    }

    /*
    /// What is a better name?
    pub async fn read_at2(&self, pos: u64, len: usize) -> io::Result<IoBufMut> {
        let op = Op::read_at2(self.fd.fd(), pos, len).unwrap();
        op.read2().await
    }
    */

    /// Close the file
    pub async fn close(self) -> io::Result<()> {
        self.fd.close().await;
        Ok(())
    }
}
