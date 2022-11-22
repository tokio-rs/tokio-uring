use super::{buffers::FixedBuffers, FixedBuf};

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

/// An indexed collection of I/O buffers pre-registered with the kernel.
///
/// `FixedBufRegistry` allows the application to manage a collection of buffers
/// allocated in memory, that can be registered in the current `tokio-uring`
/// context using the [`register`] method.
///
/// A `FixedBufRegistry` value is a lightweight handle for a collection of
/// allocated buffers. Cloning of a `FixedBufRegistry` creates a new reference to
/// the same collection of buffers.
///
/// The buffers of the collection are not deallocated until:
/// - all `FixedBufRegistry` references to the collection have been dropped;
/// - all [`FixedBuf`] handles to individual buffers in the collection have
///   been dropped, including the buffer handles owned by any I/O operations
///   in flight;
/// - The `tokio-uring` [`Runtime`] the buffers are registered with
///   has been dropped.
///
/// [`register`]: Self::register
/// [`Runtime`]: crate::Runtime
#[derive(Clone)]
pub struct FixedBufRegistry {
    inner: Rc<RefCell<FixedBuffers>>,
}

impl FixedBufRegistry {
    /// Creates a new collection of buffers from the provided allocated vectors.
    ///
    /// The buffers are assigned 0-based indices in the order of the iterable
    /// input parameter. The returned collection takes up to [`UIO_MAXIOV`]
    /// buffers from the input. Any items in excess of that amount are silently
    /// dropped, unless the input iterator produces the vectors lazily.
    ///
    /// [`UIO_MAXIOV`]: libc::UIO_MAXIOV
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::buf::fixed::FixedBufRegistry;
    /// use std::iter;
    ///
    /// let registry = FixedBufRegistry::new(iter::repeat(vec![0; 4096]).take(10));
    /// ```
    pub fn new(bufs: impl IntoIterator<Item = Vec<u8>>) -> Self {
        FixedBufRegistry {
            inner: Rc::new(RefCell::new(FixedBuffers::new(bufs.into_iter()))),
        }
    }

    /// Registers the buffers with the kernel.
    ///
    /// This method must be called in the context of a `tokio-uring` runtime.
    /// The registration persists for the lifetime of the runtime, unless
    /// revoked by the [`unregister`] method. Dropping the
    /// `FixedBufRegistry` instance this method has been called on does not revoke
    /// the registration or deallocate the buffers.
    ///
    /// [`unregister`]: Self::unregister
    ///
    /// This call can be blocked in the kernel to complete any operations
    /// in-flight on the same `io-uring` instance. The application is
    /// recommended to register buffers before starting any I/O operations.
    ///
    /// # Errors
    ///
    /// If a collection of buffers is currently registered in the context
    /// of the `tokio-uring` runtime this call is made in, the function returns
    /// an error.
    pub fn register(&self) -> io::Result<()> {
        crate::io::register_buffers(&self.inner)
    }

    /// Unregisters this collection of buffers.
    ///
    /// This method must be called in the context of a `tokio-uring` runtime,
    /// where the buffers should have been previously registered.
    ///
    /// This operation invalidates any `FixedBuf` handles checked out from
    /// this registry instance. Continued use of such handles in I/O
    /// operations may result in an error.
    ///
    /// # Errors
    ///
    /// If another collection of buffers is currently registered in the context
    /// of the `tokio-uring` runtime this call is made in, the function returns
    /// an error. Calling `unregister` when no `FixedBufRegistry` is currently
    /// registered on this runtime also returns an error.
    pub fn unregister(&self) -> io::Result<()> {
        crate::io::unregister_buffers(&self.inner)
    }

    /// Returns a buffer identified by the specified index for use by the
    /// application, unless the buffer is already in use.
    ///
    /// The buffer is released to be available again once the
    /// returned `FixedBuf` handle has been dropped. An I/O operation
    /// using the buffer takes ownership of it and returns it once completed,
    /// preventing shared use of the buffer while the operation is in flight.
    pub fn check_out(&self, index: usize) -> Option<FixedBuf> {
        let mut inner = self.inner.borrow_mut();
        inner.check_out(index).map(|(iovec, init_len)| {
            debug_assert!(index <= u16::MAX as usize);
            // Safety: the validity of iovec and init_len is ensured by
            // FixedBuffers::check_out
            unsafe { FixedBuf::new(Rc::clone(&self.inner), iovec, init_len, index as u16) }
        })
    }
}
