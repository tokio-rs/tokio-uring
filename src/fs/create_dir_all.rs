use futures_util::future::LocalBoxFuture;
use rustix::fs::StatxFlags;
use std::io;
use std::path::Path;

/// Recursively create a directory and all of its parent components if they are missing.
///
/// # Examples
///
/// ```no_run
/// tokio_uring::start(async {
///     tokio_uring::fs::create_dir_all("/some/dir").await.unwrap();
/// });
/// ```
pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .create(path.as_ref())
        .await
}

/// A builder used to create directories in various manners, based on uring async operations.
///
/// This builder supports the Linux specific option `mode` and may support `at` in the future.
#[derive(Debug)]
pub struct DirBuilder {
    inner: fs_imp::DirBuilder,
    recursive: bool,
}

impl Default for DirBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DirBuilder {
    /// Creates a new set of options with default mode/security settings for all
    /// platforms and also non-recursive.
    ///
    /// # Examples
    ///
    /// ```
    /// let builder = tokio_uring::fs::DirBuilder::new();
    /// ```
    #[must_use]
    pub fn new() -> DirBuilder {
        DirBuilder {
            inner: fs_imp::DirBuilder::new(),
            recursive: false,
        }
    }

    /// Indicates that directories should be created recursively, creating all
    /// parent directories. Parents that do not exist are created with the same
    /// security and permissions settings.
    ///
    /// This option defaults to `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut builder = tokio_uring::fs::DirBuilder::new();
    /// builder.recursive(true);
    /// ```
    #[must_use]
    pub fn recursive(&mut self, recursive: bool) -> &mut Self {
        self.recursive = recursive;
        self
    }

    /// Sets the mode to create new directories with. This option defaults to 0o777.
    ///
    /// This option defaults to 0o777.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut builder = tokio_uring::fs::DirBuilder::new();
    /// builder.mode(0o700);
    /// ```
    #[must_use]
    pub fn mode(&mut self, mode: u32) -> &mut Self {
        self.inner.set_mode(mode);
        self
    }

    /// Creates the specified directory with the options configured in this
    /// builder.
    ///
    /// It is considered an error if the directory already exists unless
    /// recursive mode is enabled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// tokio_uring::start(async {
    ///     let path = "/tmp/foo/bar/baz";
    ///     tokio_uring::fs::DirBuilder::new()
    ///         .recursive(true)
    ///         .mode(0o700) // user-only mode: drwx------
    ///         .create(path).await.unwrap();
    ///
    ///     // TODO change with tokio_uring version
    ///     assert!(std::fs::metadata(path).unwrap().is_dir());
    /// })
    /// ```
    pub async fn create<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self._create(path.as_ref()).await
    }

    async fn _create(&self, path: &Path) -> io::Result<()> {
        if self.recursive {
            self.recurse_create_dir_all(path).await
        } else {
            self.inner.mkdir(path).await
        }
    }

    // This recursive function is very closely modeled after the std library version.
    //
    // A recursive async function requires a Boxed Future. TODO There may be an implementation that
    // is less costly in terms of heap allocations. Maybe a non-recursive version is possible given
    // we even know the path separator for Linux. Or maybe expand the first level to avoid
    // recursion when only the first level of the directory needs to be built. For now, this serves
    // its purpose.

    fn recurse_create_dir_all<'a>(&'a self, path: &'a Path) -> LocalBoxFuture<io::Result<()>> {
        Box::pin(async move {
            if path == Path::new("") {
                return Ok(());
            }

            match self.inner.mkdir(path).await {
                Ok(()) => return Ok(()),
                Err(ref e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(_) if is_dir(path).await => return Ok(()),
                Err(e) => return Err(e),
            }
            match path.parent() {
                Some(p) => self.recurse_create_dir_all(p).await?,
                None => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "failed to create whole tree",
                    ));
                    /* TODO build own allocation free error some day like the std library does.
                    return Err(io::const_io_error!(
                        io::ErrorKind::Uncategorized,
                        "failed to create whole tree",
                    ));
                    */
                }
            }
            match self.inner.mkdir(path).await {
                Ok(()) => Ok(()),
                Err(_) if is_dir(path).await => Ok(()),
                Err(e) => Err(e),
            }
        })
    }
}

// TODO this DirBuilder and this fs_imp module is modeled after the std library's. Here there is
// only Linux supported so is it worth to continue this separation?

mod fs_imp {
    use crate::runtime::driver::op::Op;
    use rustix::fs::Mode;
    use std::path::Path;

    #[derive(Debug)]
    pub struct DirBuilder {
        mode: Mode,
    }

    impl DirBuilder {
        pub fn new() -> DirBuilder {
            DirBuilder { mode: 0o777.into() }
        }

        pub async fn mkdir(&self, p: &Path) -> std::io::Result<()> {
            Op::make_dir(p, self.mode)?.await
        }

        pub fn set_mode(&mut self, mode: u32) {
            self.mode = mode.into();
        }
    }
}

// Returns true if the path represents a directory.
//
// Uses one asynchronous uring call to determine this.
async fn is_dir<P: AsRef<Path>>(path: P) -> bool {
    let mut builder = crate::fs::StatxBuilder::new();
    if builder.mask(StatxFlags::TYPE).pathname(path).is_err() {
        return false;
    }

    let res = builder.statx().await;
    match res {
        Ok(statx) => (u32::from(statx.stx_mode) & libc::S_IFMT) == libc::S_IFDIR,
        Err(_) => false,
    }
}
