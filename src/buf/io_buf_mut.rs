use crate::driver::ProvidedBuf;

use std::{ops, cmp};

pub struct IoBufMut {
    kind: Kind,
}

enum Kind {
    /// A vector-backed buffer
    Vec(Vec<u8>),

    /// Buffer pool backed buffer. The pool is managed by io-uring.
    Pool(ProvidedBuf),
}

pub struct SliceMut {
    buf: IoBufMut,
    begin: usize,
    end: usize,
}

impl IoBufMut {
    pub fn with_capacity(cap: usize) -> IoBufMut {
        IoBufMut {
            kind: Kind::Vec(Vec::with_capacity(cap)),
        }
    }

    pub fn from_vec(src: Vec<u8>) -> IoBufMut {
        IoBufMut { kind: Kind::Vec(src) }
    }

    pub(crate) fn from_provided(buf: ProvidedBuf) -> IoBufMut {
        IoBufMut {
            kind: Kind::Pool(buf)
        }
    }

    pub fn slice(self, range: impl ops::RangeBounds<usize>) -> SliceMut {
        let (begin, end) = super::range(range, self.capacity());
        assert!(begin <= self.len());

        SliceMut {
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

impl ops::Deref for IoBufMut {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.vec().deref()
    }
}

impl ops::DerefMut for IoBufMut {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.vec_mut().deref_mut()
    }
}

impl SliceMut {
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

    /// Set the slice's end point
    pub fn set_end(&mut self, new_end: usize) {
        assert!(new_end >= self.begin);
        assert!(new_end <= self.buf.capacity());

        self.end = new_end;
    }

    pub fn capacity(&self) -> usize {
        self.end - self.begin
    }

    pub fn get_inner_ref(&self) -> &IoBufMut {
        &self.buf
    }

    pub fn get_inner_mut(&mut self) -> &mut IoBufMut {
        &mut self.buf
    }

    pub fn into_inner(self) -> IoBufMut {
        self.buf
    }
}

impl ops::Deref for SliceMut {
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

impl ops::DerefMut for SliceMut {
    fn deref_mut(&mut self) -> &mut [u8] {
        let end = cmp::min(self.buf.len(), self.end);

        if self.begin >= end {
            &mut self.buf[self.begin..self.begin]
        } else {
            &mut self.buf[self.begin..end]
        }
    }
}

impl From<IoBufMut> for SliceMut {
    fn from(src: IoBufMut) -> SliceMut {
        src.slice(..)
    }
}
