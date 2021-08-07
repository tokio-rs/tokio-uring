use crate::buf::IoBufMut;

/// Reads bytes at the specified offset from a source.
///
/// This trait is analogous to the [`std::io::Read`] trait, but integrates with
/// the asynchronous task system.
///
/// [`std::io::Read`]: std::io::Read
#[async_trait::async_trait(?Send)]
pub trait AsyncReadAt {
    /// Read some bytes at the specified offset from the file into the specified
    /// buffer, returning how many bytes were read.
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
    /// use tokio_uring::io::AsyncReadAt;
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
    async fn read_at<T: IoBufMut>(&self, buf: T, pos: u64) -> crate::BufResult<usize, T>;
}

/// Reads bytes from a source.
///
/// This trait is analogous to the [`std::io::Read`] trait, but integrates with
/// the asynchronous task system.
///
/// [`std::io::Read`]: std::io::Read
#[async_trait::async_trait(?Send)]
pub trait AsyncRead {
    /// Read some bytes from the file into the specified buffer, returning how
    /// many bytes were read.
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
    /// use tokio_uring::io::AsyncReadAt;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::start(async {
    ///         // Connect to a peer
    ///         let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///         let buf = vec![0u8; 128];
    ///
    ///         // Read some data.
    ///         let (res, buffer) = stream.read(buf).await;
    ///         let n = result?;
    ///
    ///         println!("The bytes: {:?}", &buffer[..n]);
    ///
    ///         // Close the file
    ///         stream.close().await?;
    ///         Ok(())
    ///     })
    /// }
    /// ```
    async fn read<T: IoBufMut>(&self, buf: T) -> crate::BufResult<usize, T>;
}
