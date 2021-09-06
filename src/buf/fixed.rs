//! Buffers pre-registered with the kernel.
//!
//! This module provides facilities for registering in-memory buffers with
//! the `tokio-uring` runtime. Operations like [`File::read_fixed_at`][rfa] and
//! [`File::write_fixed_at`][wfa] make use of buffers pre-mapped by
//! the kernel to reduce per-I/O overhead.
//! The [`BufRegistry::register`] method is used to register a collection of
//! buffers with the kernel; it must be called before any of the [`FixedBuf`]
//! handles to the collection's buffers can be used with I/O operations.
//!
//! [rfa]: crate::fs::File::read_fixed_at
//! [wfa]: crate::fs::File::write_fixed_at

use super::{IoBuf, IoBufMut};
use crate::driver::{self, Buffers};

use std::cell::RefCell;
use std::io;
use std::mem::ManuallyDrop;
use std::rc::Rc;

/// An indexed collection of I/O buffers pre-registered with the kernel.
///
/// `BufRegistry` allows the application to manage a collection of buffers
/// allocated in memory, that can be registered in the current `tokio-uring`
/// context using the [`register`] method.
///
/// [`register`]: Self::register
///
/// A `BufRegistry` value is a lightweight handle for a collection of
/// allocated buffers. Cloning of a `BufRegistry` creates a new reference to
/// the same collection of buffers.
///
/// The buffers of the collection are not deallocated until:
/// - all `BufRegistry` references to the collection have been dropped;
/// - all [`FixedBuf`] handles to individual buffers in the collection have
///   been dropped, including the buffer handles owned by any I/O operations
///   in flight;
/// - every `tokio-uring` runtime the buffers have been registered in
///   has terminated.
#[derive(Clone)]
pub struct BufRegistry {
    inner: Rc<RefCell<Buffers>>,
}

/// A unique handle to a memory buffer that can be pre-registered with
/// the kernel for `io-uring` operations.
///
/// `FixedBuf` handles can be obtained from a [`BufRegistry`] collection.
/// For each buffer, only a single `FixedBuf` handle can be either used by the
/// application code or owned by an I/O operation at any given time,
/// thus avoiding data races between `io-uring` operations in flight and
/// the application accessing buffer data.
pub struct FixedBuf {
    registry: Rc<RefCell<Buffers>>,
    buf: ManuallyDrop<Vec<u8>>,
    index: u16,
}

impl BufRegistry {
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
    /// use tokio_uring::buf::fixed::BufRegistry;
    /// use std::iter;
    ///
    /// let registry = BufRegistry::new(iter::repeat(vec![0; 4096]).take(10));
    /// ```
    pub fn new(bufs: impl IntoIterator<Item = Vec<u8>>) -> Self {
        BufRegistry {
            inner: Rc::new(RefCell::new(Buffers::new(bufs.into_iter()))),
        }
    }

    /// Registers the buffers with the kernel.
    ///
    /// This method must be called in the context of a `tokio-uring` runtime.
    /// The registration persists for the lifetime of the runtime, unless
    /// revoked by the [`unregister`] method. Dropping the
    /// `BufRegistry` instance this method has been called on does not revoke
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
        driver::register_buffers(&self.inner)
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
    /// an error. Calling `unregister` when no `BufRegistry` is currently
    /// registered on this runtime also returns an error.
    pub fn unregister(&self) -> io::Result<()> {
        driver::unregister_buffers(&self.inner)
    }

    /// Returns a buffer identified by the specified index for use by the
    /// application, unless the buffer is already in use.
    ///
    /// The buffer is released to be checked out again once the
    /// returned `FixedBuf` handle has been dropped. An I/O operation
    /// using the buffer takes ownership of it and returns it once completed,
    /// preventing shared use of the buffer while the operation is in flight.
    pub fn check_out(&self, index: usize) -> Option<FixedBuf> {
        let mut inner = self.inner.borrow_mut();
        inner.check_out(index).map(|(iovec, init_len)| {
            debug_assert!(index <= u16::MAX as usize);
            let buf = unsafe { Vec::from_raw_parts(iovec.iov_base as _, init_len, iovec.iov_len) };
            FixedBuf {
                registry: Rc::clone(&self.inner),
                buf: ManuallyDrop::new(buf),
                index: index as u16,
            }
        })
    }
}

impl Drop for FixedBuf {
    fn drop(&mut self) {
        let mut registry = self.registry.borrow_mut();
        registry.check_in(self.index as usize, self.buf.len());
    }
}

impl FixedBuf {
    pub(crate) fn buf_index(&self) -> u16 {
        self.index
    }
}

unsafe impl IoBuf for FixedBuf {
    fn stable_ptr(&self) -> *const u8 {
        self.buf.as_ptr()
    }

    fn bytes_init(&self) -> usize {
        self.buf.len()
    }

    fn bytes_total(&self) -> usize {
        self.buf.capacity()
    }
}

unsafe impl IoBufMut for FixedBuf {
    fn stable_mut_ptr(&mut self) -> *mut u8 {
        self.buf.as_mut_ptr()
    }

    unsafe fn set_init(&mut self, pos: usize) {
        if self.buf.len() < pos {
            self.buf.set_len(pos)
        }
    }
}
