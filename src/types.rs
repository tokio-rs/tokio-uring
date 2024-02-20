use std::fmt::{Debug, Display};

/// A specialized `Result` type for `io-uring` operations with buffers.
///
/// This type is used as a return value for asynchronous `io-uring` methods that
/// require passing ownership of a buffer to the runtime. When the operation
/// completes, the buffer is returned both in the success tuple and as part of the error.
///
/// # Examples
///
/// ```no_run
/// use tokio_uring::fs::File;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     tokio_uring::start(async {
///         // Open a file
///         let file = File::open("hello.txt").await?;
///
///         let buf = vec![0; 4096];
///         // Read some data, the buffer is passed by ownership and
///         // submitted to the kernel. When the operation completes,
///         // we get the buffer back.
///         let (n, buf) = file.read_at(buf, 0).await?;
///
///         // Display the contents
///         println!("{:?}", &buf[..n]);
///
///         Ok(())
///     })
/// }
/// ```
pub type Result<T, B> = std::result::Result<(T, B), Error<B>>;

/// A specialized `Error` type for `io-uring` operations with buffers.
pub struct Error<B>(pub std::io::Error, pub B);
impl<T> Debug for Error<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl<T> Display for Error<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<T> std::error::Error for Error<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl<B> Error<B> {
    /// Applies a function to the contained buffer, returning a new `BufError`.
    pub fn map<F, U>(self, f: F) -> Error<U>
    where
        F: FnOnce(B) -> U,
    {
        Error(self.0, f(self.1))
    }
}

mod private {
    pub trait Sealed {}
}
impl<T, B> private::Sealed for std::result::Result<T, B> {}

/// A Specialized trait for mapping over the buffer in both sides of a Result<T,B>
pub trait MapResult<B, U>: private::Sealed {
    /// The result type after applying the map operation
    type Output;
    /// Apply a function over the buffer on both sides of the result
    fn map_buf(self, f: impl FnOnce(B) -> U) -> Self::Output;
}

/// Adapter trait to convert result::Result<T, E> to crate::Result<T, B> where E can be
/// converted to std::io::Error.
pub trait WithBuffer<T, B>: private::Sealed {
    /// Insert a buffer into each side of the result
    fn with_buffer(self, buf: B) -> T;
}
impl<T, B, U> MapResult<B, U> for Result<T, B> {
    type Output = Result<T, U>;
    fn map_buf(self, f: impl FnOnce(B) -> U) -> Self::Output {
        match self {
            Ok((r, b)) => Ok((r, f(b))),
            Err(e) => Err(e.map(f)),
        }
    }
}

/// Adaptor implementation for Result<T, E> to Result<T, B>.
impl<T, B, E> WithBuffer<crate::Result<T, B>, B> for std::result::Result<T, E>
where
    E: Into<std::io::Error>,
{
    fn with_buffer(self, buf: B) -> Result<T, B> {
        match self {
            Ok(res) => Ok((res, buf)),
            Err(e) => Err(crate::Error(e.into(), buf)),
        }
    }
}
