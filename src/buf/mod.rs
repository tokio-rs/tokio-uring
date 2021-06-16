use crate::driver::ProvidedBuf;

use std::{ops, cmp};

/// Wrapper around a buffer type
pub struct IoBuf {
    kind: Kind,
}

enum Kind {
    /// A vector-backed buffer
    Vec(Vec<u8>),

    /// Buffer pool backed buffer. The pool is managed by io-uring.
    Pool(ProvidedBuf),
}

pub struct Slice {
    buf: IoBuf,
    begin: usize,
    end: usize,
}

impl IoBuf {
    pub fn with_capacity(cap: usize) -> IoBuf {
        IoBuf {
            kind: Kind::Vec(Vec::with_capacity(cap)),
        }
    }

    pub(crate) fn from_provided(buf: ProvidedBuf) -> IoBuf {
        IoBuf {
            kind: Kind::Pool(buf)
        }
    }

    pub fn slice(self, range: impl ops::RangeBounds<usize>) -> Slice {
        use core::ops::Bound;

        let cap = self.capacity();

        let begin = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };

        assert!(begin < cap);
        assert!(begin <= self.len());

        let end = match range.end_bound() {
            Bound::Included(&n) => n.checked_add(1).expect("out of range"),
            Bound::Excluded(&n) => n,
            Bound::Unbounded => cap,
        };

        assert!(end <= cap);

        Slice {
            buf: self,
            begin,
            end,
        }
    }

    fn vec(&self) -> &Vec<u8> {
        match &self.kind {
            Kind::Vec(v) => v,
            Kind::Pool(v) => v.vec(),
        }
    }

    fn vec_mut(&mut self) -> &mut Vec<u8> {
        match &mut self.kind {
            Kind::Vec(v) => v,
            Kind::Pool(v) => v.vec_mut(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.vec().capacity()
    }

    pub fn clear(&mut self) {
        self.vec_mut().clear()
    }

    pub fn truncate(&mut self, len: usize) {
        self.vec_mut().truncate(len)
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        self.vec_mut().set_len(new_len)
    }
}

impl ops::Deref for IoBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.vec().deref()
    }
}

impl ops::DerefMut for IoBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.vec_mut().deref_mut()
    }
}

impl Slice {
    /// Offset in the underlying buffer at which this slice starts.
    pub fn begin(&self) -> usize {
        self.begin
    }

    /// Set the slice's offset
    pub fn set_begin(&mut self, new_begin: usize) {
        assert!(new_begin <= self.buf.capacity());
        self.begin = new_begin;
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn set_end(&mut self, new_end: usize) {
        assert!(new_end >= self.begin);
        assert!(new_end <= self.buf.capacity());

        self.end = new_end;
    }

    pub fn clear(&mut self) {
        self.buf.truncate(self.begin);
    }

    pub fn capacity(&self) -> usize {
        self.end - self.begin
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        self.buf.set_len(self.begin + new_len);
    }

    pub(crate) unsafe fn assume_init(&mut self, n: usize) {
        if self.len() < n {
            self.set_len(n);
        }
    }

    pub fn into_buf(self) -> IoBuf {
        self.buf
    }
}

impl ops::Deref for Slice {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        let end = cmp::min(self.buf.len(), self.end);

        if self.begin >= end {
            &self.buf[self.begin..self.begin]
        } else {
            &self.buf[self.begin..end]
        }
    }
}

impl ops::DerefMut for Slice {
    fn deref_mut(&mut self) -> &mut [u8] {
        let end = cmp::min(self.buf.len(), self.end);

        if self.begin >= end {
            &mut self.buf[self.begin..self.begin]
        } else {
            &mut self.buf[self.begin..end]
        }
    }
}

impl From<IoBuf> for Slice {
    fn from(src: IoBuf) -> Slice {
        src.slice(..)
    }
}

impl Default for Slice {
    fn default() -> Slice {
        Slice {
            buf: IoBuf::with_capacity(0),
            begin: 0,
            end: 0,
        }
    }
}
