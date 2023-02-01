use libc::iovec;

use super::handle::AllocatedBuf;

// Abstracts management of fixed buffers in a buffer allocator.
pub(super) unsafe trait AllocatableBuffers: FixedBuffers {
    /// Releases the buffer back to its allocator implementation so it can be reused.
    ///
    /// Sets the indexed buffer's state to free and records the updated length
    /// of its initialized part.
    ///
    /// # Panics
    ///
    /// The buffer addressed must be in a valid state for the allocator to reclaim
    /// otherwise this function may panic.
    ///
    /// # Safety
    ///
    /// While the implementation of this method typically does not need to
    /// do anything unsafe, freeing a buffer not provided by this allocator is undefined,
    /// the caller must ensure that the bytes in the buffer
    /// are initialized up to the specified length.
    unsafe fn free(&mut self, buf: AllocatedBuf);
}

// Abstracts access of fixed buffers in a buffer allocator.
pub(crate) trait FixedBuffers {
    // Provides access to the raw buffers as a slice of iovec.
    // used during registration to hand buffers to the kernel.
    fn iovecs(&self) -> &[iovec];
}
