use super::File;
use crate::runtime::driver::op::Op;
use std::io;
use std::path::Path;

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
        Op::statx(Some(&self.fd), None, flags, mask)?.await
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
            file: Some(self),
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
    StatxBuilder::new().statx_path(path).await
}

/// A builder used to make a uring statx(2) call.
///
/// This builder supports the `flags` and `mask` options and can be finished with a call to
/// `statx()` or to `statx_path().
///
/// See StatxBuilder::new for more details.
#[derive(Debug)]
pub struct StatxBuilder<'a> {
    file: Option<&'a File>,
    flags: i32,
    mask: u32,
}

impl<'a> Default for StatxBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> StatxBuilder<'a> {
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
    ///         .statx_path("foo.txt").await.unwrap();
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
    pub fn new() -> StatxBuilder<'a> {
        StatxBuilder {
            file: None,
            flags: libc::AT_EMPTY_PATH,
            mask: libc::STATX_ALL,
        }
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
    ///         .statx_path("foo.txt").await.unwrap();
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
    ///         .statx_path("foo.txt").await.unwrap();
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
    /// use tokio_uring::fs::File;
    ///
    /// tokio_uring::start(async {
    ///     let f = File::create("foo.txt").await.unwrap();
    ///
    ///     // Fetch file metadata
    ///     let statx = f.statx_builder()
    ///         .mask(libc::STATX_TYPE)
    ///         .statx().await.unwrap();
    ///
    ///     // Close the file
    ///     f.close().await.unwrap();
    /// })
    /// ```
    pub async fn statx(&self) -> io::Result<libc::statx> {
        Op::statx(self.file.map(|x| &x.fd), None, self.flags, self.mask)?.await
    }

    // TODO should the statx() terminator be renamed to something like nopath()
    // and the statx_path() be renamed to path(AsRef<Path>)?

    /// Returns the metadata requested for the given path. The path can be absolute or relative.
    ///
    /// When the path is relative, it works against the open file discriptor if it has one
    /// else it works against the current working directory.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// tokio_uring::start(async {
    ///     // Fetch metadata for relative path against current working directory.
    ///     let statx = tokio_uring::fs::StatxBuilder::new()
    ///         .mask(libc::STATX_BASIC_STATS)
    ///         .statx_path("foo.txt").await.unwrap();
    /// })
    /// ```
    ///
    /// # Examples
    ///
    /// ```no_run
    /// tokio_uring::start(async {
    ///     // This shows the power of combining an open file, presumably a directory, and the relative
    ///     // path to have the statx operation return the meta data for the child of the opened directory
    ///     // descriptor.
    ///     let dir_path = "/home/ubuntu";
    ///     let rel_path = "./work";
    ///
    ///     let open_dir = tokio_uring::fs::File::open(dir_path).await.unwrap();
    ///
    ///     // Fetch metadata for relative path against open directory.
    ///     let statx_res = open_dir.statx_builder().statx_path(rel_path).await;
    ///
    ///     // Close the directory descriptor.
    ///     open_dir.close().await.unwrap();
    ///
    ///     // Use the relative path's statx information.
    ///     let _ = statx_res.unwrap();
    /// })
    /// ```
    pub async fn statx_path<P: AsRef<Path>>(&self, path: P) -> io::Result<libc::statx> {
        // It's almost an accident there are two terminators for the StatxBuilder, one that doesn't
        // take a path, and one that does.
        //
        // Because the <AsRef<Path>> is of unknown size, it can't be stored in the builder without
        // boxing it, and the conversion to CString can't be done without potentially returning an
        // error and returning an error can't be done by a method that returns a &mut self, and
        // trying to return a Result<&mut self> may have led to trouble, but what really seemed
        // like trouble was using the CString field in the other terminator. Have to look into this
        // again.
        use crate::io::cstr;

        let path = cstr(path.as_ref())?;
        Op::statx(self.file.map(|x| &x.fd), Some(path), self.flags, self.mask)?.await
    }
}

// TODO consider replacing this with a Statx struct with useful helper methods.
/// Returns two bools, is_dir and is_regfile.
///
/// They both can't be true at the same time and there are many reasons they may both be false.
#[allow(dead_code)]
pub async fn is_dir_regfile<P: AsRef<Path>>(path: P) -> (bool, bool) {
    let res = crate::fs::StatxBuilder::new()
        .mask(libc::STATX_TYPE)
        .statx_path(path)
        .await;
    match res {
        Ok(statx) => (
            (u32::from(statx.stx_mode) & libc::S_IFMT) == libc::S_IFDIR,
            (u32::from(statx.stx_mode) & libc::S_IFMT) == libc::S_IFREG,
        ),
        Err(_) => (false, false),
    }
}
