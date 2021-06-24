use crate::buf::IoBuf;

/// Io-uring compatible mutable buffer
///
/// TODO: remove `Unpin` requirement.
pub unsafe trait IoBufMut: IoBuf {
    /// Returns a pointer to the memory that does not change if the value is
    /// moved.
    fn stable_mut_ptr(&mut self) -> *mut u8;

    unsafe fn set_init(&mut self, pos: usize);
}

unsafe impl IoBufMut for Vec<u8> {
    fn stable_mut_ptr(&mut self) -> *mut u8 {
        self.as_mut_ptr()
    }

    unsafe fn set_init(&mut self, init_len: usize) {
        if self.len() < init_len {
            self.set_len(init_len);
        }
    }
}
