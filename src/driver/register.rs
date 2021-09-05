use crate::runtime::CONTEXT;

use libc::{iovec, UIO_MAXIOV};
use std::cell::RefCell;
use std::cmp;
use std::io;
use std::mem;
use std::ptr;
use std::rc::Rc;
use std::slice;

pub(crate) struct Buffers {
    raw_vecs: ptr::NonNull<iovec>,
    buf_states: Vec<BufState>,
    orig_cap: usize,
}

enum BufState {
    Free { init_len: usize },
    CheckedOut,
}

impl Buffers {
    pub(crate) fn new(bufs: impl Iterator<Item = Vec<u8>>) -> Self {
        let bufs = bufs.take(cmp::min(UIO_MAXIOV as usize, 65_536));
        let (size_hint, _) = bufs.size_hint();
        let mut iovecs = Vec::with_capacity(size_hint);
        let mut buf_states = Vec::with_capacity(size_hint);
        for mut buf in bufs {
            iovecs.push(iovec {
                iov_base: buf.as_mut_ptr() as *mut _,
                iov_len: buf.capacity(),
            });
            buf_states.push(BufState::Free {
                init_len: buf.len(),
            });
            mem::forget(buf);
        }
        debug_assert_eq!(iovecs.len(), buf_states.len());
        let raw_vecs = unsafe { ptr::NonNull::new_unchecked(iovecs.as_mut_ptr()) };
        let orig_cap = iovecs.capacity();
        mem::forget(iovecs);
        Buffers {
            raw_vecs,
            buf_states,
            orig_cap,
        }
    }

    pub(crate) fn check_out(&mut self, index: usize) -> Option<(iovec, usize)> {
        let iovecs_ptr = self.raw_vecs;
        self.buf_states
            .get_mut(index)
            .and_then(|state| match *state {
                BufState::Free { init_len } => {
                    *state = BufState::CheckedOut;
                    // Safety: the allocated array under the pointer is valid
                    // for the lifetime of self, the index is inside the array
                    // as checked by Vec::get_mut above, called on the array of
                    // buf_states that has the same length.
                    let iovec = unsafe { iovecs_ptr.as_ptr().add(index).read() };
                    Some((iovec, init_len))
                }
                BufState::CheckedOut => None,
            })
    }

    pub(crate) fn check_in(&mut self, index: usize, init_len: usize) {
        let state = self
            .buf_states
            .get_mut(index)
            .expect("buffer index must be valid");
        debug_assert!(matches!(state, BufState::CheckedOut));
        *state = BufState::Free { init_len };
    }

    fn iovecs(&self) -> &[iovec] {
        // Safety: the raw_vecs pointer is valid for the lifetime of self,
        // the slice length is valid by construction.
        unsafe { slice::from_raw_parts(self.raw_vecs.as_ptr(), self.buf_states.len()) }
    }
}

impl Drop for Buffers {
    fn drop(&mut self) {
        let iovecs = unsafe {
            Vec::from_raw_parts(self.raw_vecs.as_ptr(), self.buf_states.len(), self.orig_cap)
        };
        for (i, iovec) in iovecs.iter().enumerate() {
            match self.buf_states[i] {
                BufState::Free { init_len } => {
                    let ptr = iovec.iov_base as *mut u8;
                    let cap = iovec.iov_len;
                    let v = unsafe { Vec::from_raw_parts(ptr, init_len, cap) };
                    mem::drop(v);
                }
                BufState::CheckedOut => unreachable!("all buffers must be checked in"),
            }
        }
    }
}

pub(crate) fn register_buffers(buffers: &Rc<RefCell<Buffers>>) -> io::Result<()> {
    CONTEXT.with(|cx| {
        cx.with_driver_mut(|driver| {
            driver
                .uring
                .submitter()
                .register_buffers(buffers.borrow().iovecs())?;
            driver.buffers = Some(Rc::clone(buffers));
            Ok(())
        })
    })
}

pub(crate) fn unregister_buffers(buffers: &Rc<RefCell<Buffers>>) -> io::Result<()> {
    CONTEXT.with(|cx| {
        cx.with_driver_mut(|driver| {
            if let Some(currently_registered) = &driver.buffers {
                if Rc::ptr_eq(buffers, currently_registered) {
                    driver.uring.submitter().unregister_buffers()?;
                    driver.buffers = None;
                    return Ok(());
                }
            }
            Err(io::Error::new(
                io::ErrorKind::Other,
                "not currently registered",
            ))
        })
    })
}
