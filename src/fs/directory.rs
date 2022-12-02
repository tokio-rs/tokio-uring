use crate::runtime::driver::op::Op;
use std::io;
use std::path::Path;

/// # Examples
///
/// ```no_run
/// use tokio_uring::fs::create_dir;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     tokio_uring::start(async {
///         create_dir("/some/dir").await?;
///         Ok::<(), std::io::Error>(())
///     })?;
///     Ok(())
/// }
/// ```
pub async fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let op = Op::make_dir(path.as_ref())?;
    op.await?;

    Ok(())
}

/// Removes an empty directory.
///
/// # Examples
///
/// ```no_run
/// use tokio_uring::fs::remove_dir;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     tokio_uring::start(async {
///         remove_dir("/some/dir").await?;
///         Ok::<(), std::io::Error>(())
///     })?;
///     Ok(())
/// }
/// ```
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    Op::unlink_dir(path.as_ref())?.await
}
