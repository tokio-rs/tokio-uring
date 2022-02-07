use std::{
    alloc::{alloc, dealloc, Layout},
    io,
    ops::{Deref, DerefMut},
    path::Path,
};

use crate::{
    buf::{IoBuf, IoBufMut},
    driver::{Op, SharedFd},
};

use super::OpenOptions;

/// An aligned buffer used to perform io on a `DmaFile`.
#[derive(Debug)]
pub struct DmaBuffer {
    cap: usize,
    len: usize,
    align: usize,
    data: *mut u8,
}

impl DmaBuffer {
    /// Allocates an aligned buffer.
    pub(crate) fn new(cap: usize, align: usize) -> DmaBuffer {
        let layout = Layout::from_size_align(cap, align).unwrap();
        let data = unsafe { alloc(layout) };
        Self {
            data,
            cap,
            align,
            len: 0,
        }
    }

    /// Sets the internal length of the buffer. The caller must ensure that the memory is
    /// initialized until `new_len` before calling.
    pub unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(new_len <= self.cap);
        self.len = new_len;
    }

    /// Returns the number of initialized bytes in the buffer.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns the capacity for this `DmaBuffer`.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Returns the remining capacity in the buffer.
    pub fn remaining(&self) -> usize {
        self.capacity() - self.len()
    }

    /// Returns a raw pointer to the buffer's data.
    pub fn as_ptr(&self) -> *const u8 {
        self.data as *const _
    }

    /// Returns an unsafe mutable pointer to the buffer's data.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data
    }

    /// Extends `self` with the content of `other`.
    /// Panics if `self` doesn't have enough capacity left to contain `other`.
    pub fn extend_from_slice(&mut self, other: &[u8]) {
        assert!(other.len() <= self.remaining());

        let buf = unsafe { std::slice::from_raw_parts_mut(self.data.add(self.len()), other.len()) };
        buf.copy_from_slice(other);
        self.len += other.len();
    }
}

impl Deref for DmaBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { std::slice::from_raw_parts(self.data, self.len()) }
    }
}

impl DerefMut for DmaBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { std::slice::from_raw_parts_mut(self.data, self.len()) }
    }
}

unsafe impl IoBuf for DmaBuffer {
    fn stable_ptr(&self) -> *const u8 {
        self.as_ptr()
    }

    fn bytes_init(&self) -> usize {
        self.len()
    }

    fn bytes_total(&self) -> usize {
        self.capacity()
    }
}

unsafe impl IoBufMut for DmaBuffer {
    fn stable_mut_ptr(&mut self) -> *mut u8 {
        self.as_mut_ptr()
    }

    unsafe fn set_init(&mut self, pos: usize) {
        if self.len() < pos {
            self.set_len(pos);
        }
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.cap, self.align).unwrap();
        unsafe { dealloc(self.data, layout) }
    }
}

/// A `DmaFile` is similar to a `File`, but it is openened with the `O_DIRECT` file in order to
/// perform direct IO.
///
/// Any IO operation done on a `DmaFile` must satisfy the alignement requirements given by
/// `DmaFile::alignment()`. Failing to meet this requirement will return a
///
/// # Examples
///
/// Creates a new file and write data to it:
///
/// ```no_run
/// use tokio_uring::fs::DmaFile;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     tokio_uring::start(async {
///         // Open a file
///         let file = DmaFile::create("hello.txt").await?;
///
///         // Allocate an aligned buffer
///         let mut buffer = file.alloc_dma_buffer(4096);
///
///         let data = b"hello world";
///         buffer[..data.len()].copy_from_slice(data);
///
///         // Write some data
///         let (res, _) = file.write_at(buffer, 0).await;
///         let n = res?;
///
///         println!("wrote {} bytes", n);
///
///         // Close the file
///         file.close().await?;
///
///         Ok(())
///     })
/// }
/// ```
pub struct DmaFile {
    pub(crate) fd: SharedFd,
    pub(crate) alignment: usize,
}

impl DmaFile {
    /// Attempts to open a file in read-only mode.
    ///
    /// See the [`OpenOptions::open`] method for more details.
    ///
    /// # Errors
    ///
    /// This function will return an error if `path` does not already exist.
    /// Other errors may also be returned according to [`OpenOptions::open_dma`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::DmaFile;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         let f = DmaFile::open("foo.txt").await?;
    ///
    ///         // Close the file
    ///         f.close().await?;
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub async fn open(path: impl AsRef<Path>) -> io::Result<DmaFile> {
        OpenOptions::new().read(true).open_dma(path).await
    }

    /// Opens a file in write-only mode.
    ///
    /// This function will create a file if it does not exist,
    /// and will truncate it if it does.
    ///
    /// See the [`OpenOptions::open_dma`] function for more details.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::DmaFile;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         let f = DmaFile::create("foo.txt").await?;
    ///
    ///         // Close the file
    ///         f.close().await?;
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub async fn create(path: impl AsRef<Path>) -> io::Result<DmaFile> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open_dma(path)
            .await
    }

    /// Aligns `value` up to the memory alignement requirement for this file.
    pub fn align_up(&self, value: usize) -> usize {
        (value + self.alignment - 1) & !(self.alignment - 1)
    }

    /// Aligns `value` down to the memory alignement requirement for this file.
    pub fn align_down(&self, value: usize) -> usize {
        value & !(self.alignment - 1)
    }

    /// Return the alignement requirement for this file. The returned alignement value can be used
    /// to allocate a buffer to use with this file:
    ///
    /// ```no_run
    /// use tokio_uring::fs::DmaFile;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         let file = DmaFile::open("my_file.rs").await?;
    ///         assert_eq!(file.alignment(), 4096);
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub fn alignment(&self) -> usize {
        self.alignment
    }

    /// Returns whether the provided buffer has the correct alignment to perform I/O operations
    /// with on this file.
    ///
    /// This is only on issue if the buffer was created for a file on a different file system,
    /// for which the alignment requirement for direct I/O are different.
    pub fn can_use_buffer(&self, buffer: &DmaBuffer) -> bool {
        (buffer.data as usize) % self.alignment == 0
    }

    /// Returns a buffer that is aligned for direct read and write operations on this file.
    ///
    /// Using this buffer with any file other than the one it was created for may not work, on
    /// alignement of the buffer and the file must be checked. See [`Self::can_use_buffer`] method
    /// for more information.
    pub fn alloc_dma_buffer(&self, cap: usize) -> DmaBuffer {
        DmaBuffer::new(cap, self.alignment)
    }

    /// Read some bytes at the specified offset from the file into the specified
    /// buffer, returning how many bytes were read.
    ///
    /// Direct I/O require the offset to be aligned // TODO: To what boundaries?
    ///
    /// # Return
    ///
    /// The method returns the operation result and the same buffer value passed
    /// as an argument.
    ///
    /// If the method returns [`Ok(n)`], then the read was successful. A nonzero
    /// `n` value indicates that the buffer has been filled with `n` bytes of
    /// data from the file. If `n` is `0`, then one of the following happened:
    ///
    /// 1. The specified offset is the end of the file.
    /// 2. The buffer specified was 0 bytes in length.
    ///
    /// It is not an error if the returned value `n` is smaller than the buffer
    /// size, even when the file contains enough data to fill the buffer.
    ///
    /// # Errors
    ///
    /// If this function encounters any form of I/O or other error, an error
    /// variant will be returned. The buffer is returned on error.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::File;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         let f = File::open("foo.txt").await?;
    ///         let buffer = vec![0; 10];
    ///
    ///         // Read up to 10 bytes
    ///         let (res, buffer) = f.read_at(buffer, 0).await;
    ///         let n = res?;
    ///
    ///         println!("The bytes: {:?}", &buffer[..n]);
    ///
    ///         // Close the file
    ///         f.close().await?;
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub async fn read_at(&self, buf: DmaBuffer, pos: u64) -> crate::BufResult<usize, DmaBuffer> {
        // Submit the read operation
        let op = Op::read_at(&self.fd, buf, pos).unwrap();
        op.read().await
    }

    /// Writes the content of `buf` at the page aligned offset `pos`.
    pub async fn write_at(&self, buf: DmaBuffer, pos: u64) -> crate::BufResult<usize, DmaBuffer> {
        // Submit the read operation
        let op = Op::write_at(&self.fd, buf, pos).unwrap();
        op.write().await
    }

    /// Closes the file.
    ///
    /// The method completes once the close operation has completed,
    /// guaranteeing that resources associated with the file have been released.
    ///
    /// If `close` is not called before dropping the file, the file is closed in
    /// the background, but there is no guarantee as to **when** the close
    /// operation will complete.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::File;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         // Open the file
    ///         let f = File::open("foo.txt").await?;
    ///         // Close the file
    ///         f.close().await?;
    ///
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub async fn close(self) -> io::Result<()> {
        self.fd.close().await;
        Ok(())
    }
}
