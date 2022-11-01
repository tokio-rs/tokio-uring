use crate::buf::{IoBuf, IoBufMut};

use std::cmp;
use std::ops;

/// An owned view into a contiguous sequence of bytes.
///
/// This is similar to Rust slices (`&buf[..]`) but owns the underlying buffer.
/// This type is useful for performing io-uring read and write operations using
/// a subset of a buffer.
///
/// Slices are created using [`IntoSlice::slice`].
///
/// # Examples
///
/// Creating a slice
///
/// ```
/// use tokio_uring::buf::IntoSlice;
///
/// let buf = b"hello world".to_vec();
/// let slice = buf.slice(..5);
///
/// assert_eq!(&slice[..], b"hello");
/// ```
pub struct Slice<T> {
    buf: T,
    begin: usize,
    end: usize,
}

/// Slicing for I/O buffers.
///
/// This trait provides a uniform way to produce owned slices and sub-slices
/// of buffers used in `io-uring` operations.
/// Because buffers are passed by ownership to the runtime, Rust's slice API
/// (`&buf[..]`) cannot be used. Instead, `tokio-uring` provides an owned slice
/// API: [`slice()`]. The method takes ownership of the buffer and returns a
/// [`Slice`] type that tracks the requested index range. The `Slice` type
/// itself implements this trait, converting to another `Slice` with the
/// specified sub-range.
///
/// [`slice()`]: Self::slice
pub trait IntoSlice: Sized {
    /// The buffer type underlying the produced slices.
    type Buf: IoBuf;

    /// Returns a view of the buffer with the specified range.
    /// The effective range bounds are computed against the potentially offset
    /// buffer view that this method is called on, which may itself be a slice.
    ///
    /// This method is similar to Rust's slicing (`&buf[..]`), but takes
    /// ownership of the buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::buf::IntoSlice;
    ///
    /// let buf = b"hello world".to_vec();
    /// buf.slice(5..10);
    /// ```
    fn slice(self, range: impl ops::RangeBounds<usize>) -> Slice<Self::Buf>;

    /// Converts the buffer into a [`Slice`] covering its full extent.
    ///
    /// This method is used internally by the `io-uring` operations and the
    /// end user does not need to call it directly.
    /// The implementation should be equivalent to `self.slice(..)`, but
    /// can be optimized by skipping range checks.
    fn into_full_slice(self) -> Slice<Self::Buf> {
        self.slice(..)
    }
}

impl<T> Slice<T> {
    fn new(buf: T, begin: usize, end: usize) -> Slice<T> {
        Slice { buf, begin, end }
    }

    /// Offset in the underlying buffer at which this slice starts.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::buf::IntoSlice;
    ///
    /// let buf = b"hello world".to_vec();
    /// let slice = buf.slice(1..5);
    ///
    /// assert_eq!(1, slice.begin());
    /// ```
    pub fn begin(&self) -> usize {
        self.begin
    }

    /// Ofset in the underlying buffer at which this slice ends.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::buf::IntoSlice;
    ///
    /// let buf = b"hello world".to_vec();
    /// let slice = buf.slice(1..5);
    ///
    /// assert_eq!(5, slice.end());
    /// ```
    pub fn end(&self) -> usize {
        self.end
    }

    /// Gets a reference to the underlying buffer.
    ///
    /// This method escapes the slice's view.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::buf::IntoSlice;
    ///
    /// let buf = b"hello world".to_vec();
    /// let slice = buf.slice(..5);
    ///
    /// assert_eq!(slice.get_ref(), b"hello world");
    /// assert_eq!(&slice[..], b"hello");
    /// ```
    pub fn get_ref(&self) -> &T {
        &self.buf
    }

    /// Gets a mutable reference to the underlying buffer.
    ///
    /// This method escapes the slice's view.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::buf::IntoSlice;
    ///
    /// let buf = b"hello world".to_vec();
    /// let mut slice = buf.slice(..5);
    ///
    /// slice.get_mut()[0] = b'b';
    ///
    /// assert_eq!(slice.get_mut(), b"bello world");
    /// assert_eq!(&slice[..], b"bello");
    /// ```
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.buf
    }

    /// Unwraps this `Slice`, returning the underlying buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::buf::IntoSlice;
    ///
    /// let buf = b"hello world".to_vec();
    /// let slice = buf.slice(..5);
    ///
    /// let buf = slice.into_inner();
    /// assert_eq!(buf, b"hello world");
    /// ```
    pub fn into_inner(self) -> T {
        self.buf
    }
}

impl<T: IoBuf> ops::Deref for Slice<T> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        let buf_bytes = super::deref(&self.buf);
        let end = cmp::min(self.end, buf_bytes.len());
        &buf_bytes[self.begin..end]
    }
}

impl<T: IoBufMut> ops::DerefMut for Slice<T> {
    fn deref_mut(&mut self) -> &mut [u8] {
        let buf_bytes = super::deref_mut(&mut self.buf);
        let end = cmp::min(self.end, buf_bytes.len());
        &mut buf_bytes[self.begin..end]
    }
}

impl<T: IoBuf> Slice<T> {
    /// Returns a raw pointer to the vector’s buffer at the slice offset.
    pub(crate) fn stable_ptr(&self) -> *const u8 {
        super::deref(&self.buf)[self.begin..].as_ptr()
    }

    /// Number of initialized bytes, counted from the slice's beginning.
    pub(crate) fn bytes_init(&self) -> usize {
        ops::Deref::deref(self).len()
    }

    /// Total size of the slice view, including uninitialized memory, if any.
    pub(crate) fn bytes_total(&self) -> usize {
        self.end - self.begin
    }
}

impl<T: IoBufMut> Slice<T> {
    /// Returns a raw mutable pointer to the vector’s buffer at the slice offset.
    pub(crate) fn stable_mut_ptr(&mut self) -> *mut u8 {
        super::deref_mut(&mut self.buf)[self.begin..].as_mut_ptr()
    }

    /// Updates the number of initialized bytes in the buffer,
    /// counting from the slice beginning.
    ///
    /// If the specified `pos` is greater than the value previously returned
    /// by [`Slice::bytes_init`], it becomes the new value as returned by
    /// `Slice::bytes_init`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that all bytes starting at `stable_mut_ptr()` up
    /// to `self.begin()` + `pos` are initialized and owned by the buffer.
    pub(crate) unsafe fn set_init(&mut self, pos: usize) {
        self.buf.set_init(self.begin + pos);
    }
}

impl<T: IoBuf> IntoSlice for T {
    type Buf = T;

    fn slice(self, range: impl ops::RangeBounds<usize>) -> Slice<T> {
        use core::ops::Bound;

        let begin = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1).expect("out of range"),
            Bound::Unbounded => 0,
        };

        let bytes_total = self.bytes_total();

        assert!(begin <= bytes_total);

        let end = match range.end_bound() {
            Bound::Included(&n) => n.checked_add(1).expect("out of range"),
            Bound::Excluded(&n) => n,
            Bound::Unbounded => bytes_total,
        };

        assert!(end <= bytes_total);
        assert!(begin <= self.bytes_init());

        Slice::new(self, begin, end)
    }

    fn into_full_slice(self) -> Slice<T> {
        let end = self.bytes_total();
        Slice::new(self, 0, end)
    }
}

impl<T: IoBuf> IntoSlice for Slice<T> {
    type Buf = T;

    fn slice(self, range: impl ops::RangeBounds<usize>) -> Slice<T> {
        use core::ops::Bound;

        let begin = match range.start_bound() {
            Bound::Included(&n) => self.begin.checked_add(n).expect("out of range"),
            Bound::Excluded(&n) => self
                .begin
                .checked_add(n)
                .and_then(|x| x.checked_add(1))
                .expect("out of range"),
            Bound::Unbounded => self.begin,
        };

        assert!(begin <= self.end);

        let end = match range.end_bound() {
            Bound::Included(&n) => self
                .begin
                .checked_add(n)
                .and_then(|x| x.checked_add(1))
                .expect("out of range"),
            Bound::Excluded(&n) => self.begin.checked_add(n).expect("out of range"),
            Bound::Unbounded => self.end,
        };

        assert!(end <= self.end);
        assert!(begin <= self.buf.bytes_init());

        Slice::new(self.buf, begin, end)
    }

    fn into_full_slice(self) -> Slice<T> {
        self
    }
}

#[cfg(test)]
mod test {
    use super::IntoSlice;
    use std::mem;

    #[test]
    fn can_deref_slice_into_uninit_buf() {
        let buf = Vec::with_capacity(10).slice(..);
        let _ = buf.stable_ptr();
        assert_eq!(buf.bytes_init(), 0);
        assert_eq!(buf.bytes_total(), 10);
        assert!(buf[..].is_empty());

        let mut v = Vec::with_capacity(10);
        v.push(42);
        let mut buf = v.slice(..);
        let _ = buf.stable_mut_ptr();
        assert_eq!(buf.bytes_init(), 1);
        assert_eq!(buf.bytes_total(), 10);
        assert_eq!(mem::replace(&mut buf[0], 0), 42);
        buf.copy_from_slice(&[43]);
        assert_eq!(&buf[..], &[43]);
    }
}
