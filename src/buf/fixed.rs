//! Buffers pre-registered with the kernel
use super::{IoBuf, IoBufMut};
use crate::driver::{self, Buffers};

use std::cell::RefCell;
use std::io;
use std::mem::ManuallyDrop;
use std::rc::Rc;

#[derive(Clone)]
pub struct BufRegistry {
    inner: Rc<RefCell<Buffers>>,
}

pub struct FixedBuf {
    registry: Rc<RefCell<Buffers>>,
    buf: ManuallyDrop<Vec<u8>>,
    index: u16,
}

impl BufRegistry {
    pub fn new(bufs: impl IntoIterator<Item = Vec<u8>>) -> Self {
        BufRegistry {
            inner: Rc::new(RefCell::new(Buffers::new(bufs.into_iter()))),
        }
    }

    pub fn register(&self) -> io::Result<()> {
        driver::register_buffers(Rc::clone(&self.inner))
    }

    pub fn check_out(&mut self, index: usize) -> Option<FixedBuf> {
        let mut inner = self.inner.borrow_mut();
        inner.check_out(index).map(|(iovec, init_len)| {
            debug_assert!(index <= u16::MAX as usize);
            let buf = unsafe { Vec::from_raw_parts(iovec.iov_base as _, init_len, iovec.iov_len) };
            FixedBuf {
                registry: Rc::clone(&self.inner),
                buf: ManuallyDrop::new(buf),
                index: index as u16,
            }
        })
    }
}

impl Drop for FixedBuf {
    fn drop(&mut self) {
        let mut registry = self.registry.borrow_mut();
        registry.check_in(self.index as usize, self.buf.len());
    }
}

impl FixedBuf {
    pub(crate) fn buf_index(&self) -> u16 {
        self.index
    }
}

unsafe impl IoBuf for FixedBuf {
    fn stable_ptr(&self) -> *const u8 {
        self.buf.as_ptr()
    }

    fn bytes_init(&self) -> usize {
        self.buf.len()
    }

    fn bytes_total(&self) -> usize {
        self.buf.capacity()
    }
}

unsafe impl IoBufMut for FixedBuf {
    fn stable_mut_ptr(&mut self) -> *mut u8 {
        self.buf.as_mut_ptr()
    }

    unsafe fn set_init(&mut self, pos: usize) {
        if self.buf.len() < pos {
            self.buf.set_len(pos)
        }
    }
}
