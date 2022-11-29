use libc::iovec;

// Abstracts management of fixed buffers in a buffer registry.
pub(crate) trait FixedBuffers {
    // Provides access to the raw buffers as a slice of iovec.
    fn iovecs(&self) -> &[iovec];

    // Sets the indexed buffer's state to free and records the updated length
    // of its initialized part. The buffer addressed must be in the checked out
    // state, otherwise this function will panic.
    fn check_in(&mut self, buf_index: u16, init_len: usize);
}
