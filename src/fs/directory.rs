use crate::runtime::driver::op::Op;
use std::io;
use std::path::Path;

/// Creates a directory on the local filesystem.
///
/// # Errors
///
/// This function will return an error in the following situations, but is not
/// limited to just these cases:
///
/// * User lacks permissions to create a directory at `path`
///      * [`io::ErrorKind`] would be set to `PermissionDenied`
/// * A parent of the given path doesn't exist.
///      * [`io::ErrorKind`] would be set to `NotFound` or `NotADirectory`
/// * `path` already exists.
///      * [`io::ErrorKind`] would be set to `AlreadyExists`
///
/// [`ErrorKind`]: std::io::ErrorKind
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
/// This will only remove empty directories with no children. If you want to destroy the entire
/// contents of a directory, you may try [`remove_dir_all`] which uses the standard Tokio executor.
/// There currently is no implementation of `remove_dir_all` in tokio-uring.
///
/// [`remove_dir_all`]: https://docs.rs/tokio/latest/tokio/fs/fn.remove_dir_all.html
///
/// # Errors
///
/// This function will return an error in the following situations, but is not
/// limited to just these cases:
///
/// * `path` doesn't exist.
///      * [`io::ErrorKind`] would be set to `NotFound`
/// * `path` isn't a directory.
///      * [`io::ErrorKind`] would be set to `NotADirectory`
/// * The user lacks permissions to modify/remove the directory at the provided `path`.
///      * [`io::ErrorKind`] would be set to `PermissionDenied`
/// * The directory isn't empty.
///      * [`io::ErrorKind`] would be set to `DirectoryNotEmpty`
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
