use super::handle::CheckedOutBuf;
use super::{FixedBuf, FixedBuffers};

use libc::{iovec, UIO_MAXIOV};
use std::cell::RefCell;
use std::cmp;
use std::io;
use std::mem;
use std::ptr;
use std::rc::Rc;
use std::slice;

/// An indexed collection of I/O buffers pre-registered with the kernel.
///
/// `FixedBufRegistry` allows the application to manage a collection of buffers
/// allocated in memory, that can be registered in the current `tokio-uring`
/// context using the [`register`] method. The buffers are accessed by their
/// indices using the [`check_out`] method.
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
/// [`check_out`]: Self::check_out
/// [`Runtime`]: crate::Runtime
#[derive(Clone)]
pub struct FixedBufRegistry {
    inner: Rc<RefCell<Inner>>,
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
            inner: Rc::new(RefCell::new(Inner::new(bufs.into_iter()))),
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
        crate::io::register_buffers(Rc::clone(&self.inner) as _)
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
        crate::io::unregister_buffers(Rc::clone(&self.inner) as _)
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
        inner.check_out(index).map(|data| {
            let registry = Rc::clone(&self.inner);
            // Safety: the validity of buffer data is ensured by
            // Inner::check_out
            unsafe { FixedBuf::new(registry, data) }
        })
    }
}

// Internal state shared by FixedBufRegistry and FixedBuf handles.
struct Inner {
    // Pointer to an allocated array of iovec records referencing
    // the allocated buffers. The number of initialized records is the
    // same as the length of the states array.
    raw_bufs: ptr::NonNull<iovec>,
    // State information on the buffers. Indices in this array correspond to
    // the indices in the array at raw_bufs.
    states: Vec<BufState>,
    // Original capacity of raw_bufs as a Vec.
    orig_cap: usize,
}

// State information of a buffer in the registry,
enum BufState {
    // The buffer is not in use.
    // The field records the length of the initialized part.
    Free { init_len: usize },
    // The buffer is checked out.
    // Its data are logically owned by the FixedBuf handle,
    // which also keeps track of the length of the initialized part.
    CheckedOut,
}

impl Inner {
    fn new(bufs: impl Iterator<Item = Vec<u8>>) -> Self {
        let bufs = bufs.take(cmp::min(UIO_MAXIOV as usize, u16::MAX as usize));
        let (size_hint, _) = bufs.size_hint();
        let mut iovecs = Vec::with_capacity(size_hint);
        let mut states = Vec::with_capacity(size_hint);
        for mut buf in bufs {
            iovecs.push(iovec {
                iov_base: buf.as_mut_ptr() as *mut _,
                iov_len: buf.capacity(),
            });
            states.push(BufState::Free {
                init_len: buf.len(),
            });
            mem::forget(buf);
        }
        debug_assert_eq!(iovecs.len(), states.len());

        // Safety: Vec::as_mut_ptr never returns null
        let raw_bufs = unsafe { ptr::NonNull::new_unchecked(iovecs.as_mut_ptr()) };
        let orig_cap = iovecs.capacity();
        mem::forget(iovecs);
        Inner {
            raw_bufs,
            states,
            orig_cap,
        }
    }

    // If the indexed buffer is free, changes its state to checked out
    // and returns its data.
    // If the buffer is already checked out, returns None.
    fn check_out(&mut self, index: usize) -> Option<CheckedOutBuf> {
        let state = self.states.get_mut(index)?;
        let BufState::Free { init_len } = *state else {
            return None
        };

        *state = BufState::CheckedOut;

        // Safety: the allocated array under the pointer is valid
        // for the lifetime of self, the index is inside the array
        // as checked by Vec::get_mut above, called on the array of
        // states that has the same length.
        let iovec = unsafe { self.raw_bufs.as_ptr().add(index).read() };
        debug_assert!(index <= u16::MAX as usize);
        Some(CheckedOutBuf {
            iovec,
            init_len,
            index: index as u16,
        })
    }
}

impl FixedBuffers for Inner {
    fn iovecs(&self) -> &[iovec] {
        // Safety: the raw_bufs pointer is valid for the lifetime of self,
        // the length of the states array is also the length of buffers array
        // by construction.
        unsafe { slice::from_raw_parts(self.raw_bufs.as_ptr(), self.states.len()) }
    }

    fn check_in(&mut self, index: u16, init_len: usize) {
        let state = self
            .states
            .get_mut(index as usize)
            .expect("invalid buffer index");
        debug_assert!(
            matches!(state, BufState::CheckedOut),
            "the buffer must be checked out"
        );
        *state = BufState::Free { init_len };
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let iovecs = unsafe {
            Vec::from_raw_parts(self.raw_bufs.as_ptr(), self.states.len(), self.orig_cap)
        };
        for (i, iovec) in iovecs.iter().enumerate() {
            match self.states[i] {
                BufState::Free { init_len } => {
                    let ptr = iovec.iov_base as *mut u8;
                    let cap = iovec.iov_len;
                    let v = unsafe { Vec::from_raw_parts(ptr, init_len, cap) };
                    mem::drop(v);
                }
                BufState::CheckedOut => unreachable!("all buffers must be checked in"),
            }
        }
    }
}
