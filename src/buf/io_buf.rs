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
    pub fn from_vec(src: Vec<u8>) -> IoBuf {
        IoBuf { kind: Kind::Vec(src) }
    }

    pub(crate) fn from_provided(buf: ProvidedBuf) -> IoBuf {
        IoBuf {
            kind: Kind::Pool(buf)
        }
    }

    pub fn slice(self, range: impl ops::RangeBounds<usize>) -> Slice {
        let (begin, end) = super::range(range, self.len());

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
}

impl ops::Deref for IoBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.vec().deref()
    }
}

impl Slice {
    /// Offset in the underlying buffer at which this slice starts.
    pub fn begin(&self) -> usize {
        self.begin
    }

    /// Set the slice's offset
    pub fn set_begin(&mut self, new_begin: usize) {
        assert!(new_begin <= self.buf.len());
        self.begin = new_begin;
    }

    pub fn end(&self) -> usize {
        self.end
    }

    /// Set the slice's end point
    pub fn set_end(&mut self, new_end: usize) {
        assert!(new_end >= self.begin);
        assert!(new_end <= self.buf.len());

        self.end = new_end;
    }

    pub fn get_inner_ref(&self) -> &IoBuf {
        &self.buf
    }

    pub fn into_inner(self) -> IoBuf {
        self.buf
    }
}

impl ops::Deref for Slice {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf[self.begin..self.end]
    }
}

impl From<IoBuf> for Slice {
    fn from(src: IoBuf) -> Slice {
        src.slice(..)
    }
}
