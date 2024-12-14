use rustix::io_uring::iovec;

// Abstracts management of fixed buffers in a buffer registry.
pub(crate) trait FixedBuffers {
    // Provides access to the raw buffers as a slice of iovec.
    fn iovecs(&self) -> &[iovec];

    /// Sets the indexed buffer's state to free and records the updated length
    /// of its initialized part.
    ///
    /// # Panics
    ///
    /// The buffer addressed must be in the checked out state,
    /// otherwise this function may panic.
    ///
    /// # Safety
    ///
    /// While the implementation of this method typically does not need to
    /// do anything unsafe, the caller must ensure that the bytes in the buffer
    /// are initialized up to the specified length.
    unsafe fn check_in(&mut self, buf_index: u16, init_len: usize);
}
