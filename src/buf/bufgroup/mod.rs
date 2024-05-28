//! The io_uring device implements several provided-buffering mechanisms, which are all called
//! buffer groups in the liburing man pages.
//!
//! Buffer groups share a few things in common:
//!     o all provide a mechanism to seed the kernel with userland buffers for use in various
//!       read operations
//!     o all use a u16 Buffer Group ID
//!     o all use a u16 Buffer ID
//!     o all are specified in the read or receive operations by setting
//!       the IOSQE_BUFFER_SELECT bit in the sqe flags field and
//!       then identifying the buffer group id in the sqe buf_group field
//!     o all read or receive operations that used a buffer group have
//!       the IORING_CQE_F_BUFFER bit set in the cqe flags field and
//!       the buffer id chosen in the upper 16 bits of the cqe res field
//!
//! As of Oct 2022, the latest buffer group mechanism implemented by the io_uring device, and the
//! one that promises the best performance with least amount of overhead, is the buf_ring. The
//! buf_ring has several liburing man pages, the first to reference should probably be
//! io_uring_buf_ring_init(3).

use crate::buf::bufring::BufRing;

/// The buffer group ID.
///
/// The creater of a buffer group is responsible for picking a buffer group id
/// that does not conflict with other buffer group ids also being registered with the uring
/// interface.
pub(crate) type Bgid = u16;

// Future: Maybe create a bgid module with a trivial implementation of a type that tracks the next
// bgid to use. The crate's driver could do that perhaps, but there could be a benefit to tracking
// them across multiple thread's drivers. So there is flexibility in not building it into the
// driver.

/// The buffer ID. Buffer ids are assigned and used by the crate and probably are not visible
/// to the crate user.
pub(crate) type Bid = u16;

/// This tracks a buffer that has been filled in by the kernel, having gotten the memory
/// from a buffer ring, and returned to userland via a cqe entry.
pub struct BufX {
    bgroup: BufRing,
    bid: Bid,
    len: usize,
}

impl BufX {
    // # Safety
    //
    // The bid must be the buffer id supplied by the kernel as having been chosen and written to.
    // The length of the buffer must represent the length written to by the kernel.
    pub(crate) unsafe fn new(bgroup: BufRing, bid: Bid, len: usize) -> Self {
        // len will already have been checked against the buf_capacity
        // so it is guaranteed that len <= bgroup.buf_capacity.

        Self { bgroup, bid, len }
    }

    /// Return the number of bytes initialized.
    ///
    /// This value initially came from the kernel, as reported in the cqe. This value may have been
    /// modified with a call to the IoBufMut::set_init method.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Return true if this represents an empty buffer. The length reported by the kernel was 0.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the capacity of this buffer.
    #[inline]
    pub fn cap(&self) -> usize {
        self.bgroup.buf_capacity(self.bid)
    }

    /// Return a byte slice reference.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        let p = self.bgroup.stable_ptr(self.bid);
        // Safety: the pointer returned by stable_ptr is valid for the lifetime of self,
        // and self's len is set when the kernel reports the amount of data that was
        // written into the buffer.
        unsafe { std::slice::from_raw_parts(p, self.len) }
    }

    /// Return a mutable byte slice reference.
    #[inline]
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        let p = self.bgroup.stable_mut_ptr(self.bid);
        // Safety: the pointer returned by stable_mut_ptr is valid for the lifetime of self,
        // and self's len is set when the kernel reports the amount of data that was
        // written into the buffer. In addition, we hold a &mut reference to self.
        unsafe { std::slice::from_raw_parts_mut(p, self.len) }
    }

    // Future: provide access to the uninit space between len and cap if the buffer is being
    // repurposed before being dropped. The set_init below does that too.
}

impl Drop for BufX {
    fn drop(&mut self) {
        // Add the buffer back to the bgroup, for the kernel to reuse.
        // Safety: this function may only be called by the buffer's drop function.
        unsafe { self.bgroup.dropping_bid(self.bid) };
    }
}

unsafe impl crate::buf::IoBuf for BufX {
    fn stable_ptr(&self) -> *const u8 {
        self.bgroup.stable_ptr(self.bid)
    }

    fn bytes_init(&self) -> usize {
        self.len
    }

    fn bytes_total(&self) -> usize {
        self.cap()
    }
}

unsafe impl crate::buf::IoBufMut for BufX {
    fn stable_mut_ptr(&mut self) -> *mut u8 {
        self.bgroup.stable_mut_ptr(self.bid)
    }

    unsafe fn set_init(&mut self, init_len: usize) {
        if self.len < init_len {
            let cap = self.bgroup.buf_capacity(self.bid);
            assert!(init_len <= cap);
            self.len = init_len;
        }
    }
}

impl From<BufX> for Vec<u8> {
    fn from(item: BufX) -> Self {
        item.as_slice().to_vec()
    }
}
