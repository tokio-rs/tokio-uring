use crate::buf::{IoBuf, IoBufMut};

use std::ops;
pub struct Slice<T> {
    buf: T,
    begin: usize,
    end: usize,
}

impl<T> Slice<T> {
    pub(crate) fn new(buf: T, begin: usize, end: usize) -> Slice<T> {
        Slice { buf, begin, end }
    }

    /// Offset in the underlying buffer at which this slice starts.
    pub fn begin(&self) -> usize {
        self.begin
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn get_ref(&self) -> &T {
        &self.buf
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.buf
    }

    pub fn into_inner(self) -> T {
        self.buf
    }
}

impl<T: IoBuf> ops::Deref for Slice<T> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &super::deref(&self.buf)[self.begin..self.end]
    }
}

impl<T: IoBufMut> ops::DerefMut for Slice<T> {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut super::deref_mut(&mut self.buf)[self.begin..self.end]
    }
}

unsafe impl<T: IoBuf> IoBuf for Slice<T> {
    fn stable_ptr(&self) -> *const u8 {
        ops::Deref::deref(self).as_ptr()
    }

    fn bytes_init(&self) -> usize {
        ops::Deref::deref(self).len()
    }

    fn bytes_total(&self) -> usize {
        self.end - self.begin
    }
}

unsafe impl<T: IoBufMut> IoBufMut for Slice<T> {
    fn stable_mut_ptr(&mut self) -> *mut u8 {
        ops::DerefMut::deref_mut(self).as_mut_ptr()
    }

    unsafe fn set_init(&mut self, pos: usize) {
        self.buf.set_init(self.begin + pos);
    }
}
