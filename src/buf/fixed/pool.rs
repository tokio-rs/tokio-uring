use super::plumbing;
use super::FixedBuf;

use crate::runtime::driver::WeakHandle;
use crate::runtime::CONTEXT;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

/// A dynamic collection of I/O buffers pre-registered with the kernel.
///
/// `FixedBufPool` allows the application to manage a collection of buffers
/// allocated in memory, that can be registered in the current `tokio-uring`
/// context using the [`register`] method. Unlike [`FixedBufRegistry`],
/// individual buffers are not retrieved by index; instead, an available
/// buffer matching a specified capacity can be retrieved with the [`try_next`]
/// method. This allows some flexibility in managing sets of buffers with
/// different capacity tiers. The need to maintain lists of free buffers,
/// however, imposes additional runtime overhead.
///
/// A `FixedBufPool` value is a lightweight handle for a collection of
/// allocated buffers. Cloning of a `FixedBufPool` creates a new reference to
/// the same collection of buffers.
///
/// The buffers of the collection are not deallocated until:
/// - all `FixedBufPool` references to the collection have been dropped;
/// - all [`FixedBuf`] handles to individual buffers in the collection have
///   been dropped, including the buffer handles owned by any I/O operations
///   in flight;
/// - The `tokio-uring` [`Runtime`] the buffers are registered with
///   has been dropped.
///
/// [`register`]: Self::register
/// [`try_next`]: Self::try_next
/// [`FixedBufRegistry`]: super::FixedBufRegistry
/// [`Runtime`]: crate::Runtime
///
/// # Examples
///
/// ```
/// use tokio_uring::buf::fixed::FixedBufPool;
/// use tokio_uring::buf::IoBuf;
/// use std::iter;
/// use std::mem;
///
/// # #[allow(non_snake_case)]
/// # fn main() -> Result<(), std::io::Error> {
/// # use nix::sys::resource::{getrlimit, Resource};
/// # let (memlock_limit, _) = getrlimit(Resource::RLIMIT_MEMLOCK)?;
/// # let BUF_SIZE_LARGE = memlock_limit as usize / 8;
/// # let BUF_SIZE_SMALL = memlock_limit as usize / 16;
/// tokio_uring::start(async {
///     let pool = FixedBufPool::new(
///          iter::once(Vec::with_capacity(BUF_SIZE_LARGE))
///              .chain(iter::repeat_with(|| Vec::with_capacity(BUF_SIZE_SMALL)).take(2))
///      );
///
///     pool.register()?;
///
///     let buf = pool.try_next(BUF_SIZE_LARGE).unwrap();
///     assert_eq!(buf.bytes_total(), BUF_SIZE_LARGE);
///     let next = pool.try_next(BUF_SIZE_LARGE);
///     assert!(next.is_none());
///     let buf1 = pool.try_next(BUF_SIZE_SMALL).unwrap();
///     assert_eq!(buf1.bytes_total(), BUF_SIZE_SMALL);
///     let buf2 = pool.try_next(BUF_SIZE_SMALL).unwrap();
///     assert_eq!(buf2.bytes_total(), BUF_SIZE_SMALL);
///     let next = pool.try_next(BUF_SIZE_SMALL);
///     assert!(next.is_none());
///     mem::drop(buf);
///     let buf = pool.try_next(BUF_SIZE_LARGE).unwrap();
///     assert_eq!(buf.bytes_total(), BUF_SIZE_LARGE);
///
///     Ok(())
/// })
/// # }
/// ```
#[derive(Clone)]
pub struct FixedBufPool {
    inner: Rc<RefCell<plumbing::Pool>>,
    driver: WeakHandle,
}

impl FixedBufPool {
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
    /// When providing uninitialized vectors for the collection, take care to
    /// not replicate a vector with `.clone()` as that does not preserve the
    /// capacity and the resulting buffer pointer will be rejected by the kernel.
    /// This means that the following use of [`iter::repeat`] would not work:
    ///
    /// [`iter::repeat`]: std::iter::repeat
    ///
    /// ```should_panic
    /// use tokio_uring::buf::fixed::FixedBufPool;
    /// use std::iter;
    ///
    /// # #[allow(non_snake_case)]
    /// # fn main() -> Result<(), std::io::Error> {
    /// # use nix::sys::resource::{getrlimit, Resource};
    /// # let (memlock_limit, _) = getrlimit(Resource::RLIMIT_MEMLOCK)?;
    /// # let NUM_BUFFERS = std::cmp::max(memlock_limit as usize / 4096 / 8, 1);
    /// # let BUF_SIZE = 4096;
    /// tokio_uring::start(async {
    ///     let pool = FixedBufPool::new(
    ///         iter::repeat(Vec::with_capacity(BUF_SIZE)).take(NUM_BUFFERS)
    ///     );
    ///     pool.register()?;
    ///     // ...
    ///     Ok(())
    /// })
    /// # }
    /// ```
    ///
    /// Instead, create the vectors with requested capacity directly:
    ///
    /// ```
    /// use tokio_uring::buf::fixed::FixedBufPool;
    /// use std::iter;
    ///
    /// # #[allow(non_snake_case)]
    /// # fn main() -> Result<(), std::io::Error> {
    /// # use nix::sys::resource::{getrlimit, Resource};
    /// # let (memlock_limit, _) = getrlimit(Resource::RLIMIT_MEMLOCK)?;
    /// # let NUM_BUFFERS = std::cmp::max(memlock_limit as usize / 4096 / 8, 1);
    /// # let BUF_SIZE = 4096;
    /// tokio_uring::start(async {
    ///     let pool = FixedBufPool::new(
    ///         iter::repeat_with(|| Vec::with_capacity(BUF_SIZE)).take(NUM_BUFFERS)
    ///     );
    ///     pool.register()?;
    ///     // ...
    ///     Ok(())
    /// })
    /// # }
    /// ```
    pub fn new(bufs: impl IntoIterator<Item = Vec<u8>>) -> Self {
        FixedBufPool {
            inner: Rc::new(RefCell::new(plumbing::Pool::new(bufs.into_iter()))),
            driver: CONTEXT.with(|x| {
                x.handle()
                    .as_ref()
                    .expect("Not in a runtime context")
                    .into()
            }),
        }
    }

    /// Registers the buffers with the kernel.
    ///
    /// This method must be called in the context of a `tokio-uring` runtime.
    /// The registration persists for the lifetime of the runtime, unless
    /// revoked by the [`unregister`] method. Dropping the
    /// `FixedBufPool` instance this method has been called on does not revoke
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
        self.driver
            .upgrade()
            .expect("Runtime context is no longer present")
            .register_buffers(Rc::clone(&self.inner) as _)
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
    /// an error. Calling `unregister` when no `FixedBufPool` is currently
    /// registered on this runtime also returns an error.
    pub fn unregister(&self) -> io::Result<()> {
        self.driver
            .upgrade()
            .expect("Runtime context is no longer present")
            .unregister_buffers(Rc::clone(&self.inner) as _)
    }

    /// Returns a buffer of requested capacity from this pool
    /// that is not currently owned by any other [`FixedBuf`] handle.
    /// If no such free buffer is available, returns `None`.
    ///
    /// The buffer is released to be available again once the
    /// returned `FixedBuf` handle has been dropped. An I/O operation
    /// using the buffer takes ownership of it and returns it once completed,
    /// preventing shared use of the buffer while the operation is in flight.
    ///
    /// An application should not rely on any particular order
    /// in which available buffers are retrieved.
    pub fn try_next(&self, cap: usize) -> Option<FixedBuf> {
        let mut inner = self.inner.borrow_mut();
        inner.try_next(cap).map(|data| {
            let registry = Rc::clone(&self.inner);
            // Safety: the validity of buffer data is ensured by
            // plumbing::Pool::try_next
            unsafe { FixedBuf::new(registry, data) }
        })
    }
}
