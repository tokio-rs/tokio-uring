use crate::runtime::driver::op::Op;
use std::io;
use std::path::Path;

/// Creates a directory on the local filesystem.
///
/// Returns a future which resolves to unit on success, and ['std::io::Error'] on failure.
///
/// # Errors
///
/// This function will return an error in the following situations, but is not
/// limited to just these cases:
///
/// * User lacks permissions to create a directory at `path`.
/// * A parent of the given path doesn't exist.
/// * `path` already exists.
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
///
/// Returns a future which resolves to unit on success, and ['std::io::Error'] on failure.
///
/// # Errors
///
/// This function will return an error in the following situations, but is not
/// limited to just these cases:
///
/// * `path` doesn't exist.
/// * `path` isn't a directory.
/// * The user lacks permissions to modify/remove the directory at the provided `path`.
/// * The directory isn't empty.
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
