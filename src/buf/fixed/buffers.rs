use libc::{iovec, UIO_MAXIOV};
use std::cmp;
use std::mem;
use std::ptr;
use std::slice;

// Internal state shared by FixedBufRegistry and FixedBuf handles.
pub(crate) struct FixedBuffers {
    // Pointer to an allocated array of iovec records referencing
    // the allocated buffers. The number of initialized records is the
    // same as the length of the states array.
    raw_bufs: ptr::NonNull<iovec>,
    // State information on the buffers. Indices in this array correspond to
    // the indices in the array at raw_bufs.
    states: Vec<BufState>,
    // Original capacity of raw_bufs as a Vec.
    orig_cap: usize,
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

impl FixedBuffers {
    pub(crate) fn new(bufs: impl Iterator<Item = Vec<u8>>) -> Self {
        let bufs = bufs.take(cmp::min(UIO_MAXIOV as usize, 65_536));
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
        FixedBuffers {
            raw_bufs,
            states,
            orig_cap,
        }
    }

    // If the indexed buffer is free, changes its state to checked out and
    // returns its data. If the buffer is already checked out, returns None.
    pub(crate) fn check_out(&mut self, index: usize) -> Option<(iovec, usize)> {
        let iovecs_ptr = self.raw_bufs;
        self.states.get_mut(index).and_then(|state| match *state {
            BufState::Free { init_len } => {
                *state = BufState::CheckedOut;
                // Safety: the allocated array under the pointer is valid
                // for the lifetime of self, the index is inside the array
                // as checked by Vec::get_mut above, called on the array of
                // states that has the same length.
                let iovec = unsafe { iovecs_ptr.as_ptr().add(index).read() };
                Some((iovec, init_len))
            }
            BufState::CheckedOut => None,
        })
    }

    // Sets the indexed buffer's state to free and records the updated length
    // of its initialized part. The buffer addressed must be in the checked out
    // state, otherwise this function may panic.
    pub(crate) fn check_in(&mut self, index: usize, init_len: usize) {
        let state = self.states.get_mut(index).expect("invalid buffer index");
        debug_assert!(
            matches!(state, BufState::CheckedOut),
            "the buffer must be checked out"
        );
        *state = BufState::Free { init_len };
    }

    pub(crate) fn iovecs(&self) -> &[iovec] {
        // Safety: the raw_bufs pointer is valid for the lifetime of self,
        // the slice length is valid by construction.
        unsafe { slice::from_raw_parts(self.raw_bufs.as_ptr(), self.states.len()) }
    }
}

impl Drop for FixedBuffers {
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
