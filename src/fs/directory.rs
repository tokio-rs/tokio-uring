use crate::runtime::driver::op::Op;
use std::io;
use std::path::Path;

/// Creates a directory on the local filesystem.
/// Returns a future which resolves to unit on success, and ['std::io::Error'] on failure.
///
/// Preconditions:
/// 1. The specified directory must not already exist.
/// 2. The parent directory must exist.
/// 3. The user must have sufficient permissions to create the directory on the filesystem.
///
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
    Op::make_dir(path.as_ref())?.await
}

/// Removes a directory on the local filesystem.
/// Returns a future which resolves to unit on success, and ['std::io::Error'] on failure.
///
/// Preconditions:
/// 1. The specified directory must exist.
/// 2. The specified directory must be empty.
/// 3. The user must have sufficient permissions to modify/remove a directory on the filesystem.
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
