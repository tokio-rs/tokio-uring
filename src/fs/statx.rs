use super::File;
use crate::io::{cstr, SharedFd};
use crate::runtime::driver::op::Op;
use std::{ffi::CString, io, path::Path};

impl File {
    /// Returns statx(2) metadata for an open file via a uring call.
    ///
    /// The libc::statx structure returned is described in the statx(2) man page.
    ///
    /// This high level version of the statx function uses `flags` set to libc::AT_EMPTY_PATH and
    /// `mask` set to libc::STATX_ALL which are described in the same man page.
    ///
    /// More specific uring statx(2) calls can be made with the StatxBuilder.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::File;
    ///
    /// tokio_uring::start(async {
    ///     let f = File::create("foo.txt").await.unwrap();
    ///
    ///     // Fetch file metadata
    ///     let statx = f.statx().await.unwrap();
    ///
    ///     // Close the file
    ///     f.close().await.unwrap();
    /// })
    /// ```
    pub async fn statx(&self) -> io::Result<libc::statx> {
        let flags = libc::AT_EMPTY_PATH;
        let mask = libc::STATX_ALL;
        Op::statx(Some(self.fd.clone()), None, flags, mask)?.await
    }

    /// Returns a builder that can return statx(2) metadata for an open file using the uring
    /// device.
    ///
    /// `flags` and `mask` can be changed from their defaults and a `path` that is absolule or
    /// relative can also be provided.
    ///
    /// `flags` defaults to libc::AT_EMPTY_PATH.
    ///
    /// `mask` defaults to libc::STATX_ALL.
    ///
    /// Refer to statx(2) for details on the arguments and the returned value.
    ///
    /// A little from the man page:
    ///
    ///  - statx(2) uses path, dirfd, and flags to identify the target file.
    ///  - statx(2) uses mask to tell the kernel which fields the caller is interested in.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::File;
    ///
    /// tokio_uring::start(async {
    ///     let f = File::create("foo.txt").await.unwrap();
    ///
    ///     // Fetch file metadata
    ///     let statx = f.statx_builder()
    ///         .flags(libc::AT_NO_AUTOMOUNT)
    ///         .statx().await.unwrap();
    ///
    ///     // Close the file
    ///     f.close().await.unwrap();
    /// })
    /// ```
    pub fn statx_builder(&self) -> StatxBuilder {
        StatxBuilder {
            file: Some(self.fd.clone()),
            path: None,
            flags: libc::AT_EMPTY_PATH,
            mask: libc::STATX_ALL,
        }
    }
}

/// Returns statx(2) metadata for a path via a uring call.
///
/// The libc::statx structure returned is described in the statx(2) man page.
///
/// This high level version of the statx function uses `flags` set to libc::AT_EMPTY_PATH and
/// `mask` set to libc::STATX_ALL which are described in the same man page.
///
/// And this version of the function does not work on an open file descriptor can be more expedient
/// when an open file descriptor isn't necessary for other reasons anyway.
///
/// The path can be absolute or relative. A relative path is interpreted against the current
/// working direcgtory.
///
/// More specific uring statx(2) calls can be made with the StatxBuilder.
///
/// # Examples
///
/// ```no_run
/// tokio_uring::start(async {
///
///     // Fetch file metadata
///     let statx = tokio_uring::fs::statx("foo.txt").await.unwrap();
/// })
/// ```
pub async fn statx<P: AsRef<Path>>(path: P) -> io::Result<libc::statx> {
    StatxBuilder::new().pathname(path).unwrap().statx().await
}

/// A builder used to make a uring statx(2) call.
///
/// This builder supports the `flags` and `mask` options and can be finished with a call to
/// `statx()`.
///
/// See StatxBuilder::new for more details.
pub struct StatxBuilder {
    file: Option<SharedFd>,
    path: Option<CString>,
    flags: i32,
    mask: u32,
}

impl Default for StatxBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl StatxBuilder {
    /// Returns a builder to fully specify the arguments to the uring statx(2) operation.
    ///
    /// The libc::statx structure returned in described in the statx(2) man page.
    ///
    /// This builder defaults to having no open file descriptor and defaults `flags` to
    /// libc::AT_EMPTY_PATH and `mask` to libc::STATX_ALL.
    ///
    /// Refer to the man page for details about the `flags`, `mask` values and returned structure,
    /// libc::statx.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// tokio_uring::start(async {
    ///     let want_mode: u16 = 0o775;
    ///
    ///     // Fetch file metadata
    ///     let statx = tokio_uring::fs::StatxBuilder::new()
    ///         .mask(libc::STATX_MODE)
    ///         .pathname("foo.txt").unwrap()
    ///         .statx().await.unwrap();
    ///     let got_mode = statx.stx_mode & 0o7777;
    ///
    ///     if want_mode == got_mode {
    ///         println!("Success: wanted mode {want_mode:#o}, got mode {got_mode:#o}");
    ///     } else {
    ///         println!("Fail: wanted mode {want_mode:#o}, got mode {got_mode:#o}");
    ///     }
    /// })
    /// ```
    #[must_use]
    pub fn new() -> StatxBuilder {
        StatxBuilder {
            file: None,
            path: None,
            flags: libc::AT_EMPTY_PATH,
            mask: libc::STATX_ALL,
        }
    }

    /// Sets the `dirfd` option, setting or replacing the file descriptor which may be for a
    /// directory but doesn't have to be. When used with a path, it should be a directory but when
    /// used without a path, can be any file type. So `dirfd` is a bit of a misnomer but it is what
    /// the statx(2) man page calls it.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::{self, File};
    ///
    /// tokio_uring::start(async {
    ///     let dir = fs::OpenOptions::new()
    ///         .open("/home/linux")
    ///         .await.unwrap();
    ///
    ///     // Fetch file metadata
    ///     let statx = fs::StatxBuilder::new()
    ///         .dirfd(&dir)
    ///         .mask(libc::STATX_TYPE)
    ///         .pathname(".cargo").unwrap()
    ///         .statx().await.unwrap();
    ///
    ///     dir.close().await.unwrap();
    /// })
    /// ```
    #[must_use]
    pub fn dirfd(&mut self, file: &File) -> &mut Self {
        self.file = Some(file.fd.clone());
        self
    }

    /// Sets the `path` option, setting or replacing the path option to the command.
    /// The path may be absolute or relative.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::{self, File};
    ///
    /// tokio_uring::start(async {
    ///     let dir = fs::OpenOptions::new()
    ///         .open("/home/linux")
    ///         .await.unwrap();
    ///
    ///     // Fetch file metadata
    ///     let statx = fs::StatxBuilder::new()
    ///         .dirfd(&dir)
    ///         .pathname(".cargo").unwrap()
    ///         .mask(libc::STATX_TYPE)
    ///         .statx().await.unwrap();
    ///
    ///     dir.close().await.unwrap();
    /// })
    /// ```
    pub fn pathname<P: AsRef<Path>>(&mut self, path: P) -> io::Result<&mut Self> {
        self.path = Some(cstr(path.as_ref())?);
        Ok(self)
    }

    /// Sets the `flags` option, replacing the default.
    ///
    /// See statx(2) for a full description of `flags`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// tokio_uring::start(async {
    ///     // Fetch file metadata
    ///     let statx = tokio_uring::fs::StatxBuilder::new()
    ///         .flags(libc::AT_NO_AUTOMOUNT)
    ///         .pathname("foo.txt").unwrap()
    ///         .statx().await.unwrap();
    /// })
    /// ```
    #[must_use]
    pub fn flags(&mut self, flags: i32) -> &mut Self {
        self.flags = flags;
        self
    }

    /// Sets the `mask` option, replacing the default.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// tokio_uring::start(async {
    ///     // Fetch file metadata
    ///     let statx = tokio_uring::fs::StatxBuilder::new()
    ///         .mask(libc::STATX_BASIC_STATS)
    ///         .pathname("foo.txt").unwrap()
    ///         .statx().await.unwrap();
    /// })
    /// ```
    #[must_use]
    pub fn mask(&mut self, mask: u32) -> &mut Self {
        self.mask = mask;
        self
    }

    /// Returns the metadata requested for the optional open file. If no open file was provided,
    /// the metadata for the current working directory is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::{self, File};
    ///
    /// tokio_uring::start(async {
    ///     let dir = fs::OpenOptions::new()
    ///         .open("/home/linux")
    ///         .await.unwrap();
    ///
    ///     // Fetch file metadata
    ///     let statx = fs::StatxBuilder::new()
    ///         .dirfd(&dir)
    ///         .pathname(".cargo").unwrap()
    ///         .mask(libc::STATX_TYPE)
    ///         .statx().await.unwrap();
    ///
    ///     dir.close().await.unwrap();
    /// })
    /// ```
    pub async fn statx(&mut self) -> io::Result<libc::statx> {
        // TODO should the statx() terminator be renamed to something like submit()?
        let fd = self.file.take();
        let path = self.path.take();
        Op::statx(fd, path, self.flags, self.mask)?.await
    }
}

// TODO consider replacing this with a Statx struct with useful helper methods.
/// Returns two bools, is_dir and is_regfile.
///
/// They both can't be true at the same time and there are many reasons they may both be false.
#[allow(dead_code)]
pub async fn is_dir_regfile<P: AsRef<Path>>(path: P) -> (bool, bool) {
    let mut builder = crate::fs::StatxBuilder::new();
    if builder.mask(libc::STATX_TYPE).pathname(path).is_err() {
        return (false, false);
    }

    let res = builder.statx().await;
    match res {
        Ok(statx) => (
            (u32::from(statx.stx_mode) & libc::S_IFMT) == libc::S_IFDIR,
            (u32::from(statx.stx_mode) & libc::S_IFMT) == libc::S_IFREG,
        ),
        Err(_) => (false, false),
    }
}
