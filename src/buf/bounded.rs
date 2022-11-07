use super::{IoBuf, IoBufMut, Slice};

use std::ops;

/// A possibly bounded view into an owned [`IoBuf`] buffer.
///
/// Because buffers are passed by ownership to the runtime, Rust's slice API
/// (`&buf[..]`) cannot be used. Instead, `tokio-uring` provides an owned slice
/// API: [`.slice()`]. The method takes ownership of the buffer and returns a
/// [`Slice`] value that tracks the requested range.
///
/// This trait provides a generic way to use buffers and `Slice` views
/// into such buffers with `io-uring` operations.
///
/// [`.slice()`]: BoundedBuf::slice
pub trait BoundedBuf: Sized + Unpin + 'static {
    /// The type of the underlying buffer.
    type Buf: IoBuf;

    /// The type representing the range bounds of the view.
    type Bounds: ops::RangeBounds<usize>;

    /// Returns a view of the buffer with the specified range.
    ///
    /// This method is similar to Rust's slicing (`&buf[..]`), but takes
    /// ownership of the buffer. The range bounds are specified against
    /// the possibly offset beginning of the `self` view into the buffer
    /// and the end bound, if specified, must not exceed the view's total size.
    /// Note that the range may extend into the uninitialized part of the
    /// buffer, but it must start (if so bounded) in the initialized part
    /// or immediately adjacent to it.
    ///
    /// # Panics
    ///
    /// If the range is invalid with regard to the recipient's total size or
    /// the length of its initialized part, the implementation of this method
    /// should panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_uring::buf::BoundedBuf;
    ///
    /// let buf = b"hello world".to_vec();
    /// let slice = buf.slice(5..10);
    /// assert_eq!(&slice[..], b" worl");
    /// let slice = slice.slice(1..3);
    /// assert_eq!(&slice[..], b"wo");
    /// ```
    fn slice(self, range: impl ops::RangeBounds<usize>) -> Slice<Self::Buf>;

    /// Returns the range bounds for this view.
    fn bounds(&self) -> Self::Bounds;

    /// Constructs a view from an underlying buffer and range bounds.
    fn from_buf_bounds(buf: Self::Buf, bounds: Self::Bounds) -> Self;

    /// Like [`IoBuf::stable_ptr`],
    /// but possibly offset to the view's starting position.
    fn stable_ptr(&self) -> *const u8;

    /// Number of initialized bytes available via this view.
    fn bytes_init(&self) -> usize;

    /// Total size of the view, including uninitialized memory, if any.
    fn bytes_total(&self) -> usize;
}

impl<T: IoBuf> BoundedBuf for T {
    type Buf = Self;
    type Bounds = ops::RangeFull;

    fn slice(self, range: impl ops::RangeBounds<usize>) -> Slice<Self> {
        use ops::Bound;

        let begin = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1).expect("out of range"),
            Bound::Unbounded => 0,
        };

        assert!(begin < self.bytes_total());

        let end = match range.end_bound() {
            Bound::Included(&n) => n.checked_add(1).expect("out of range"),
            Bound::Excluded(&n) => n,
            Bound::Unbounded => self.bytes_total(),
        };

        assert!(end <= self.bytes_total());
        assert!(begin <= self.bytes_init());

        Slice::new(self, begin, end)
    }

    fn bounds(&self) -> Self::Bounds {
        ..
    }

    fn from_buf_bounds(buf: Self, _: ops::RangeFull) -> Self {
        buf
    }

    fn stable_ptr(&self) -> *const u8 {
        IoBuf::stable_ptr(self)
    }

    fn bytes_init(&self) -> usize {
        IoBuf::bytes_init(self)
    }

    fn bytes_total(&self) -> usize {
        IoBuf::bytes_total(self)
    }
}

/// A possibly bounded view into an owned [`IoBufMut`] buffer.
///
/// This trait provides a generic way to use mutable buffers and `Slice` views
/// into such buffers with `io-uring` operations.
pub trait BoundedBufMut: BoundedBuf<Buf = Self::BufMut> {
    /// The type of the underlying buffer.
    type BufMut: IoBufMut;

    /// Like [`IoBufMut::stable_mut_ptr`],
    /// but possibly offset to the view's starting position.
    fn stable_mut_ptr(&mut self) -> *mut u8;

    /// Like [`IoBufMut::set_init`],
    /// but the position is possibly offset to the view's starting position.
    unsafe fn set_init(&mut self, pos: usize);
}

impl<T: IoBufMut> BoundedBufMut for T {
    type BufMut = T;

    fn stable_mut_ptr(&mut self) -> *mut u8 {
        IoBufMut::stable_mut_ptr(self)
    }

    unsafe fn set_init(&mut self, pos: usize) {
        IoBufMut::set_init(self, pos)
    }
}
