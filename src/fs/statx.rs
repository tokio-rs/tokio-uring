use super::File;
use crate::runtime::driver::op::Op;
use std::io;

impl File {
    /// Metadata information about a file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::File;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         let f = File::create("foo.txt").await?;
    ///
    ///         // Fetch file metadata
    ///         let statx = f.statx().await?;
    ///
    ///         // Close the file
    ///         f.close().await?;
    ///         Ok(())
    ///     })
    /// }
    pub async fn statx(&self) -> io::Result<libc::statx> {
        Op::statx(&self.fd)?.await
    }
}
