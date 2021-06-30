use crate::buf::{IoBuf, IoBufMut};
use crate::driver::{Op, SharedFd};
use crate::fs::OpenOptions;

use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
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
    pub async fn read_at<T: IoBufMut>(&self, buf: T, pos: u64) -> crate::BufResult<usize, T> {
        // Submit the read operation
        let op = Op::read_at(&self.fd, buf, pos).unwrap();
        op.read().await
    }

    pub async fn write_at<T: IoBuf>(&self, buf: T, pos: u64) -> crate::BufResult<usize, T> {
        let op = Op::write_at(&self.fd, buf, pos).unwrap();
        op.write().await
    }

    pub async fn sync_all(&self) -> io::Result<()> {
        let op = Op::fsync(&self.fd).unwrap();
        let completion = op.await;

        completion.result?;
        Ok(())
    }

    pub async fn sync_data(&self) -> io::Result<()> {
        let op = Op::datasync(&self.fd).unwrap();
        let completion = op.await;

        completion.result?;
        Ok(())
    }

    /// Close the file
    pub async fn close(self) -> io::Result<()> {
        self.fd.close().await;
        Ok(())
    }
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}
