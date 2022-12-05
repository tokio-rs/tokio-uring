use super::FixedBuffers;
use crate::buf::{IoBuf, IoBufMut};

use libc::iovec;
use std::cell::RefCell;
use std::fmt::{self, Debug};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

// Data to construct a `FixedBuf` handle from.
pub(crate) struct CheckedOutBuf {
    // Pointer and size of the buffer.
    pub iovec: iovec,
    // Length of the initialized part.
    pub init_len: usize,
    // Buffer index.
    pub index: u16,
}

/// A unique handle to a memory buffer that can be pre-registered with
/// the kernel for `io-uring` operations.
///
/// `FixedBuf` handles can be obtained from a collection of fixed buffers,
/// either [`FixedBufRegistry`] or [`FixedBufPool`].
/// For each buffer, only a single `FixedBuf` handle can be either used by the
/// application code or owned by an I/O operation at any given time,
/// thus avoiding data races between `io-uring` operations in flight and
/// the application accessing buffer data.
///
/// [`FixedBufRegistry`]: super::FixedBufRegistry
/// [`FixedBufPool`]:     super::FixedBufPool
///
pub struct FixedBuf {
    registry: Rc<RefCell<dyn FixedBuffers>>,
    buf: ManuallyDrop<Vec<u8>>,
    index: u16,
}

impl Drop for FixedBuf {
    fn drop(&mut self) {
        let mut registry = self.registry.borrow_mut();
        // Safety: the length of the initialized data in the buffer has been
        // maintained accordingly to the safety contracts on
        // Self::new and IoBufMut.
        unsafe {
            registry.check_in(self.index, self.buf.len());
        }
    }
}

impl FixedBuf {
    // Safety: Validity constraints must apply to CheckedOutBuf members:
    // - iovec must refer to an array allocated by Vec<u8>;
    // - the array will not be deallocated until the buffer is checked in;
    // - the data in the array must be initialized up to the number of bytes
    //   given in init_len.
    pub(super) unsafe fn new(registry: Rc<RefCell<dyn FixedBuffers>>, data: CheckedOutBuf) -> Self {
        let CheckedOutBuf {
            iovec,
            init_len,
            index,
        } = data;
        let buf = Vec::from_raw_parts(iovec.iov_base as _, init_len, iovec.iov_len);
        FixedBuf {
            registry,
            buf: ManuallyDrop::new(buf),
            index,
        }
    }

    /// Index of the underlying registry buffer
    pub fn buf_index(&self) -> u16 {
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

impl Deref for FixedBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf
    }
}

impl DerefMut for FixedBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }
}

impl Debug for FixedBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FixedBuf")
            .field("buf", &*self.buf) // deref ManuallyDrop
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}
