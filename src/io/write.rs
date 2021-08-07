use crate::buf::IoBuf;

/// Attempt to write bytes at the specified offset from `buf` into the object.
#[async_trait::async_trait(?Send)]
pub trait AsyncWriteAt {
    /// Write a buffer into this file at the specified offset, returning how
    /// many bytes were written.
    ///
    /// This function will attempt to write the entire contents of `buf`, but
    /// the entire write may not succeed, or the write may also generate an
    /// error. The bytes will be written starting at the specified offset.
    ///
    /// # Return
    ///
    /// The method returns the operation result and the same buffer value passed
    /// in as an argument. A return value of `0` typically means that the
    /// underlying file is no longer able to accept bytes and will likely not be
    /// able to in the future as well, or that the buffer provided is empty.
    ///
    /// # Errors
    ///
    /// Each call to `write` may generate an I/O error indicating that the
    /// operation could not be completed. If an error is returned then no bytes
    /// in the buffer were written to this writer.
    ///
    /// It is **not** considered an error if the entire buffer could not be
    /// written to this writer.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::File;
    /// use tokio_uring::io::AsyncWriteAt;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         let file = File::create("foo.txt").await?;
    ///
    ///         // Writes some prefix of the byte string, not necessarily all of it.
    ///         let (res, _) = file.write_at(&b"some bytes"[..], 0).await;
    ///         let n = res?;
    ///
    ///         println!("wrote {} bytes", n);
    ///
    ///         // Close the file
    ///         file.close().await?;
    ///         Ok(())
    ///     })
    /// }
    /// ```
    ///
    /// [`Ok(n)`]: Ok
    async fn write_at<T: IoBuf>(&self, buf: T, pos: u64) -> crate::BufResult<usize, T>;
}

/// Attempt to write bytes from `buf` into the object.
#[async_trait::async_trait(?Send)]
pub trait AsyncWrite {
    /// Write a buffer into this file, returning how many bytes were written.
    ///
    /// This function will attempt to write the entire contents of `buf`, but
    /// the entire write may not succeed, or the write may also generate an
    /// error. The bytes will be written starting at the specified offset.
    ///
    /// # Return
    ///
    /// The method returns the operation result and the same buffer value passed
    /// in as an argument. A return value of `0` typically means that the
    /// underlying file is no longer able to accept bytes and will likely not be
    /// able to in the future as well, or that the buffer provided is empty.
    ///
    /// # Errors
    ///
    /// Each call to `write` may generate an I/O error indicating that the
    /// operation could not be completed. If an error is returned then no bytes
    /// in the buffer were written to this writer.
    ///
    /// It is **not** considered an error if the entire buffer could not be
    /// written to this writer.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio_uring::fs::File;
    /// use tokio_uring::io::AsyncWrite;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         // Connect to a peer
    ///         let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///         // Write some data.
    ///         let (res, _) = stream.write(b"hello world!").await;
    ///         let n = result?
    ///
    ///         println!("wrote {} bytes", n);
    ///
    ///         // Close the file
    ///         stream.close().await?;
    ///         Ok(())
    ///     })
    /// }
    /// ```
    ///
    /// [`Ok(n)`]: Ok
    async fn write<T: IoBuf>(&self, buf: T) -> crate::BufResult<usize, T>;
}
