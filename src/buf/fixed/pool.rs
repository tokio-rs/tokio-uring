use super::buffers::AllocatableBuffers;
use super::handle::AllocatedBuf;
use super::{FixedBuf, FixedBuffers};

use crate::buf::IoBufMut;
use crate::runtime::CONTEXT;
use libc::{iovec, UIO_MAXIOV};
use std::cell::RefCell;
use std::cmp;
use std::collections::HashMap;
use std::io;
use std::mem;
use std::ptr;
use std::rc::Rc;
use std::slice;

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
pub struct FixedBufPool<T: IoBufMut> {
    inner: Rc<RefCell<Inner<T>>>,
}

impl<T: IoBufMut> FixedBufPool<T> {
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
    /// let pool = FixedBufPool::new(
    ///     iter::repeat(Vec::with_capacity(BUF_SIZE)).take(NUM_BUFFERS)
    /// );
    ///
    /// tokio_uring::start(async {
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
    /// let pool = FixedBufPool::new(
    ///     iter::repeat_with(|| Vec::with_capacity(BUF_SIZE)).take(NUM_BUFFERS)
    /// );
    ///
    /// tokio_uring::start(async {
    ///     pool.register()?;
    ///     // ...
    ///     Ok(())
    /// })
    /// # }
    /// ```
    pub fn new(bufs: impl IntoIterator<Item = T>) -> Self {
        FixedBufPool {
            inner: Rc::new(RefCell::new(Inner::new(bufs.into_iter()))),
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
        CONTEXT.with(|x| {
            x.handle()
                .as_ref()
                .expect("Not in a runtime context")
                .register_buffers(Rc::clone(&self.inner) as _)
        })
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
        CONTEXT.with(|x| {
            x.handle()
                .as_ref()
                .expect("Not in a runtime context")
                .unregister_buffers(Rc::clone(&self.inner) as _)
        })
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
        inner.try_next(cap).map(|buf| {
            let allocator = Rc::clone(&self.inner);
            // Safety: the validity of buffer data is ensured by
            // Inner::try_next
            unsafe { FixedBuf::new(allocator, buf) }
        })
    }
}

// Internal state shared by FixedBufPool and FixedBuf handles.
struct Inner<T: IoBufMut> {
    // Pointer to an allocated array of iovec records referencing
    // the allocated buffers. The number of initialized records is the
    // same as the length of the states array.
    raw_bufs: ptr::NonNull<iovec>,
    // State information on the buffers. Indices in this array correspond to
    // the indices in the array at raw_bufs.
    states: Vec<BufState>,
    // Original capacity of raw_bufs as a Vec.
    orig_cap: usize,
    // Original buffers, kept until drop
    buffers: Vec<T>,
    // Table of head indices of the free buffer lists in each size bucket.
    free_buf_head_by_cap: HashMap<usize, u16>,
}

// State information of a buffer in the registry,
enum BufState {
    // The buffer is not in use.
    Free {
        // This field records the length of the initialized part.
        init_len: usize,
        // Index of the next buffer of the same capacity in a free buffer list, if any.
        next: Option<u16>,
    },
    // The buffer is checked out.
    // Its data are logically owned by the FixedBuf handle,
    // which also keeps track of the length of the initialized part.
    CheckedOut,
}

impl<T: IoBufMut> Inner<T> {
    fn new(bufs: impl Iterator<Item = T>) -> Self {
        // Limit the number of buffers to the maximum allowable number.
        let bufs = bufs.take(cmp::min(UIO_MAXIOV as usize, u16::MAX as usize));
        // Collect into `buffers`, which holds the backing buffers for
        // the lifetime of the pool. Using collect may allow
        // the compiler to apply collect in place specialization,
        // to avoid an allocation.
        let mut buffers = bufs.collect::<Vec<T>>();
        let mut iovecs = Vec::with_capacity(buffers.len());
        let mut states = Vec::with_capacity(buffers.len());
        let mut free_buf_head_by_cap = HashMap::new();
        for (index, buf) in buffers.iter_mut().enumerate() {
            let cap = buf.bytes_total();

            // Link the buffer as the head of the free list for its capacity.
            // This constructs the free buffer list to be initially retrieved
            // back to front, which should be of no difference to the user.
            let next = free_buf_head_by_cap.insert(cap, index as u16);

            iovecs.push(iovec {
                iov_base: buf.stable_mut_ptr() as *mut _,
                iov_len: cap,
            });
            states.push(BufState::Free {
                init_len: buf.bytes_init(),
                next,
            });
        }
        debug_assert_eq!(iovecs.len(), states.len());
        debug_assert_eq!(iovecs.len(), buffers.len());

        // Safety: Vec::as_mut_ptr never returns null
        let raw_bufs = unsafe { ptr::NonNull::new_unchecked(iovecs.as_mut_ptr()) };
        let orig_cap = iovecs.capacity();
        mem::forget(iovecs);
        Inner {
            raw_bufs,
            states,
            orig_cap,
            buffers,
            free_buf_head_by_cap,
        }
    }

    // If the free buffer list for this capacity is not empty, checks out the first buffer
    // from the list and returns its data. Otherwise, returns None.
    fn try_next(&mut self, cap: usize) -> Option<AllocatedBuf> {
        let free_head = self.free_buf_head_by_cap.get_mut(&cap)?;
        let index = *free_head as usize;
        let state = &mut self.states[index];

        let (init_len, next) = match *state {
            BufState::Free { init_len, next } => {
                *state = BufState::CheckedOut;
                (init_len, next)
            }
            BufState::CheckedOut => panic!("buffer is checked out"),
        };

        // Update the head of the free list for this capacity.
        match next {
            Some(i) => {
                *free_head = i;
            }
            None => {
                self.free_buf_head_by_cap.remove(&cap);
            }
        }

        // Safety: the allocated array under the pointer is valid
        // for the lifetime of self, a free buffer index is inside the array,
        // as also asserted by the indexing operation on the states array
        // that has the same length.
        let iovec = unsafe { self.raw_bufs.as_ptr().add(index).read() };
        debug_assert_eq!(iovec.iov_len, cap);
        Some(AllocatedBuf {
            iovec,
            init_len,
            index: index as u16,
        })
    }

    fn check_in_internal(&mut self, index: u16, init_len: usize) {
        let cap = self.iovecs()[index as usize].iov_len;
        let state = &mut self.states[index as usize];
        debug_assert!(
            matches!(state, BufState::CheckedOut),
            "the buffer must be checked out"
        );

        // Link the buffer as the new head of the free list for its capacity.
        // Recently checked in buffers will be first to be reused,
        // improving cache locality.
        let next = self.free_buf_head_by_cap.insert(cap, index);

        *state = BufState::Free { init_len, next };
    }
}

impl<T: IoBufMut> FixedBuffers for Inner<T> {
    fn iovecs(&self) -> &[iovec] {
        // Safety: the raw_bufs pointer is valid for the lifetime of self,
        // the length of the states array is also the length of buffers array
        // by construction.
        unsafe { slice::from_raw_parts(self.raw_bufs.as_ptr(), self.states.len()) }
    }
}

unsafe impl<T: IoBufMut> AllocatableBuffers for Inner<T> {
    unsafe fn free(&mut self, buf: AllocatedBuf) {
        self.check_in_internal(buf.index, buf.init_len)
    }
}

impl<T: IoBufMut> Drop for Inner<T> {
    fn drop(&mut self) {
        for (i, state) in self.states.iter().enumerate() {
            match state {
                BufState::Free { init_len, .. } => {
                    // Update buffer initalisation.
                    // The buffer is about to dropped, but this may release it
                    // from Registry ownership, rather than deallocate.
                    unsafe { self.buffers[i].set_init(*init_len) };
                }
                BufState::CheckedOut => unreachable!("all buffers must be checked in"),
            }
        }
        // Rebuild Vec<iovec>, so it's dropped
        let _ = unsafe {
            Vec::from_raw_parts(self.raw_bufs.as_ptr(), self.states.len(), self.orig_cap)
        };
    }
}
