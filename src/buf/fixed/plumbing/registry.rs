use crate::buf::fixed::{handle::CheckedOutBuf, FixedBuffers};

use libc::{iovec, UIO_MAXIOV};
use std::cmp;
use std::mem;
use std::ptr;
use std::slice;

// Internal state shared by FixedBufRegistry and FixedBuf handles.
pub(crate) struct Registry {
    // Pointer to an allocated array of iovec records referencing
    // the allocated buffers. The number of initialized records is the
    // same as the length of the states array.
    raw_bufs: ptr::NonNull<iovec>,
    // Original capacity of raw_bufs as a Vec.
    orig_cap: usize,
    // State information on the buffers. Indices in this array correspond to
    // the indices in the array at raw_bufs.
    states: Vec<BufState>,
}

// State information of a buffer in the registry,
enum BufState {
    // The buffer is not in use.
    // The field records the length of the initialized part.
    Free { init_len: usize },
    // The buffer is checked out.
    // Its data are logically owned by the FixedBuf handle,
    // which also keeps track of the length of the initialized part.
    CheckedOut,
}

impl Registry {
    pub(crate) fn new(bufs: impl Iterator<Item = Vec<u8>>) -> Self {
        let bufs = bufs.take(cmp::min(UIO_MAXIOV as usize, u16::MAX as usize));
        let (size_hint, _) = bufs.size_hint();
        let mut iovecs = Vec::with_capacity(size_hint);
        let mut states = Vec::with_capacity(size_hint);
        for mut buf in bufs {
            iovecs.push(iovec {
                iov_base: buf.as_mut_ptr() as *mut _,
                iov_len: buf.capacity(),
            });
            states.push(BufState::Free {
                init_len: buf.len(),
            });
            mem::forget(buf);
        }
        debug_assert_eq!(iovecs.len(), states.len());

        // Safety: Vec::as_mut_ptr never returns null
        let raw_bufs = unsafe { ptr::NonNull::new_unchecked(iovecs.as_mut_ptr()) };
        let orig_cap = iovecs.capacity();
        mem::forget(iovecs);
        Registry {
            raw_bufs,
            orig_cap,
            states,
        }
    }

    // If the indexed buffer is free, changes its state to checked out
    // and returns its data.
    // If the buffer is already checked out, returns None.
    pub(crate) fn check_out(&mut self, index: usize) -> Option<CheckedOutBuf> {
        let state = self.states.get_mut(index)?;
        let BufState::Free { init_len } = *state else {
            return None
        };

        *state = BufState::CheckedOut;

        // Safety: the allocated array under the pointer is valid
        // for the lifetime of self, the index is inside the array
        // as checked by Vec::get_mut above, called on the array of
        // states that has the same length.
        let iovec = unsafe { self.raw_bufs.as_ptr().add(index).read() };
        debug_assert!(index <= u16::MAX as usize);
        Some(CheckedOutBuf {
            iovec,
            init_len,
            index: index as u16,
        })
    }

    fn check_in_internal(&mut self, index: u16, init_len: usize) {
        let state = self
            .states
            .get_mut(index as usize)
            .expect("invalid buffer index");
        debug_assert!(
            matches!(state, BufState::CheckedOut),
            "the buffer must be checked out"
        );
        *state = BufState::Free { init_len };
    }
}

impl FixedBuffers for Registry {
    fn iovecs(&self) -> &[iovec] {
        // Safety: the raw_bufs pointer is valid for the lifetime of self,
        // the length of the states array is also the length of buffers array
        // by construction.
        unsafe { slice::from_raw_parts(self.raw_bufs.as_ptr(), self.states.len()) }
    }

    unsafe fn check_in(&mut self, index: u16, init_len: usize) {
        self.check_in_internal(index, init_len)
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        let iovecs = unsafe {
            Vec::from_raw_parts(self.raw_bufs.as_ptr(), self.states.len(), self.orig_cap)
        };
        for (i, iovec) in iovecs.iter().enumerate() {
            match self.states[i] {
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
