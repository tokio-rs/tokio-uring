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
    // Index of the next free buffer, if any is available.
    next_free_buf: ListIndex,
}

// State information of a buffer in the registry,
enum BufState {
    // The buffer is not in use.
    Free(FreeBufInfo),
    // The buffer is checked out.
    // Its data are logically owned by the FixedBuf handle,
    // which also keeps track of the length of the initialized part.
    CheckedOut,
}

// Data to construct a `FixedBuf` handle from.
pub(super) struct CheckedOutBuf {
    // Pointer and size of the buffer.
    pub iovec: iovec,
    // Length of the initialized part.
    pub init_len: usize,
    // Buffer index.
    pub index: u16,
}

#[derive(Copy, Clone)]
struct FreeBufInfo {
    // This field records the length of the initialized part.
    init_len: usize,
    // Index of the previous buffer in the free buffer list, if any.
    prev: ListIndex,
    // Index of the next buffer in the free buffer list, if any.
    next: ListIndex,
}

// Index reference for the free buffer list, smaller than `Option<u16>`.
#[derive(Copy, Clone, PartialEq, Eq)]
struct ListIndex(u16);

impl ListIndex {
    // io-uring does not allow registering more than UIO_MAXIOV buffers,
    // so we can use a larger value to represent absence of an index reference.
    const NONE: Self = Self(u16::MAX);

    fn get(self) -> Option<usize> {
        if self == Self::NONE {
            None
        } else {
            Some(self.0 as usize)
        }
    }
}

impl FixedBuffers {
    pub(crate) fn new(bufs: impl Iterator<Item = Vec<u8>>) -> Self {
        let bufs = bufs.take(cmp::min(UIO_MAXIOV as usize, ListIndex::NONE.0 as usize));
        let (size_hint, _) = bufs.size_hint();
        let mut iovecs = Vec::with_capacity(size_hint);
        let mut states = Vec::with_capacity(size_hint);
        let mut prev_idx = ListIndex::NONE;
        for (i, mut buf) in bufs.enumerate() {
            iovecs.push(iovec {
                iov_base: buf.as_mut_ptr() as *mut _,
                iov_len: buf.capacity(),
            });
            states.push(BufState::Free(FreeBufInfo {
                init_len: buf.len(),
                prev: prev_idx,
                next: ListIndex((i + 1) as u16),
            }));
            mem::forget(buf);
            prev_idx = ListIndex(i as u16);
        }
        debug_assert_eq!(iovecs.len(), states.len());
        let next_free_buf = if let Some(i) = prev_idx.get() {
            // Fix up the last buffer's next free index.
            let BufState::Free(FreeBufInfo { next, .. }) = &mut states[i]
                else { unreachable!() };
            *next = ListIndex::NONE;

            ListIndex(0)
        } else {
            ListIndex::NONE
        };

        // Safety: Vec::as_mut_ptr never returns null
        let raw_bufs = unsafe { ptr::NonNull::new_unchecked(iovecs.as_mut_ptr()) };
        let orig_cap = iovecs.capacity();
        mem::forget(iovecs);
        FixedBuffers {
            raw_bufs,
            states,
            orig_cap,
            next_free_buf,
        }
    }

    fn prev_free_buf_index_at(&mut self, index: usize) -> &mut ListIndex {
        match &mut self.states[index] {
            BufState::Free(FreeBufInfo { prev, .. }) => prev,
            BufState::CheckedOut => panic!("buffer is checked out"),
        }
    }

    fn next_free_buf_index_at(&mut self, index: usize) -> &mut ListIndex {
        match &mut self.states[index] {
            BufState::Free(FreeBufInfo { next, .. }) => next,
            BufState::CheckedOut => panic!("buffer is checked out"),
        }
    }

    // If the indexed buffer is free, changes its state to checked out, removes
    // the buffer from the free buffer list, and returns its data.
    // If the buffer is already checked out, returns None.
    pub(super) fn check_out(&mut self, index: usize) -> Option<CheckedOutBuf> {
        let state = self.states.get_mut(index)?;

        let FreeBufInfo {
            init_len,
            prev,
            next,
        } = match *state {
            BufState::Free(info) => {
                *state = BufState::CheckedOut;
                info
            }
            BufState::CheckedOut => return None,
        };

        // Remove the buffer from the free list.
        if let Some(i) = prev.get() {
            let next_of_prev = self.next_free_buf_index_at(i);
            *next_of_prev = next;
        } else {
            self.next_free_buf = next;
        }
        if let Some(i) = next.get() {
            let prev_of_next = self.prev_free_buf_index_at(i);
            *prev_of_next = prev;
        }

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

    // Sets the indexed buffer's state to free and records the updated length
    // of its initialized part. The buffer addressed must be in the checked out
    // state, otherwise this function will panic.
    pub(crate) fn check_in(&mut self, index: usize, init_len: usize) {
        let state = self.states.get_mut(index).expect("invalid buffer index");
        debug_assert!(
            matches!(state, BufState::CheckedOut),
            "the buffer must be checked out"
        );
        *state = BufState::Free(FreeBufInfo {
            init_len,
            prev: ListIndex::NONE,
            next: self.next_free_buf,
        });
        debug_assert!(index < ListIndex::NONE.0 as usize);
        self.next_free_buf = ListIndex(index as u16);
    }

    // If the free buffer list is not empty, checks out the first buffer
    // from the list and returns its data. Otherwise, returns None.
    pub(super) fn try_next(&mut self) -> Option<CheckedOutBuf> {
        let index = self.next_free_buf.get()?;
        let state = &mut self.states[index];

        let FreeBufInfo {
            init_len,
            prev: _prev,
            next,
        } = match *state {
            BufState::Free(info) => {
                *state = BufState::CheckedOut;
                info
            }
            BufState::CheckedOut => panic!("buffer is checked out"),
        };

        debug_assert!(_prev.get().is_none());
        self.next_free_buf = next;

        // Safety: the allocated array under the pointer is valid
        // for the lifetime of self, a free buffer index is inside the array,
        // as also asserted by the indexing operation on the states array
        // that has the same length.
        let iovec = unsafe { self.raw_bufs.as_ptr().add(index).read() };
        debug_assert!(index <= u16::MAX as usize);
        Some(CheckedOutBuf {
            iovec,
            init_len,
            index: index as u16,
        })
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
                BufState::Free(FreeBufInfo {
                    init_len,
                    prev: _,
                    next: _,
                }) => {
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
