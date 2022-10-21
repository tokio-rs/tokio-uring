//! TODO module comments
//! The uring interface implements several provided-buffering mechanisms, which are all called
//! buffer groups in the liburing man pages.
//!
//! Buffer groups share a few things in common:
//!     o all provide a mechanism to seed the kernel with userland buffers for use in various
//!       read operations
//!     o all use a u16 Buffer Group ID
//!     o all a u16 Buffer ID
//!     o all are specified in the read or receive operations by setting
//!       the IOSQE_BUFFER_SELECT bit in the sqe flags field and
//!       the identifying the buffer group id in the sqe buf_group field
//!     o all read or receive operations that used a buffer group have
//!       the IORING_CQE_F_BUFFER bit set in the cqe flags field and
//!       the buffer id chosen in the upper 16 bits of the cqe res field
//!
//! As of Oct 2022, the latest buffer group mechanism implemented by the uring interface, and the
//! one that promises the best performance with least amount of overhead is the buf_ring. The
//! buf_ring has several liburing man pages, the first to reference should probably be
//! io_uring_buf_ring_init.3.

// use crate::bufring::ring::BufRingRc;
use std::io;

/// The buffer group ID.
///
/// The creater of a buffer group is responsible for picking a buffer group id
/// that does not conflict with other buffer group ids also being registered with the uring
/// interface.
pub type Bgid = u16;

/// The buffer ID. Buffer ids are assigned and used by the crate and probably are not visible
/// to the crate user.
pub(crate) type Bid = u16;

/// BufXGroup is the trait that BufX uses to interface back to the Buffer Group
/// that instantiated it. And it is the trait the BufX instance uses to report back when the BufX
/// instance is being dropped.
pub trait Group {
    /// Return the buffer group ID.
    fn bgid(&self) -> Bgid;

    /// TODO
    fn buf_capacity(&self, bid: Bid) -> usize;

    /// TODO
    fn stable_ptr(&self, bid: Bid) -> *const u8;

    /// TODO
    fn stable_mut_ptr(&mut self, bid: Bid) -> *mut u8;

    /// TODO
    ///
    /// # Safety
    ///
    /// dropping_bid should only be called by the buffer's drop function
    /// because once called, the buffer may be given back to the kernel for reuse.
    unsafe fn dropping_bid(&self, bid: Bid);

    // Needed by the operation that polls, not by BufX itself.
    // Maybe a second level of trait? The dropping_bid is only needed by BufX,
    // not by anything else either. But the compiler is nice enough to let this
    // function signature be self referential. So maybe leave well enough alone.
    /// TODO
    fn get_buf<G>(&self, res: u32, flags: u32, buf_ring: G) -> io::Result<BufX<G>>
    where
        G: Group;

    /* not quite
    /// TODO
    fn get_buf2<G>(self, res: u32, flags: u32) -> io::Result<BufX<G>>
    where
        G: Group;
    */
}

/// This tracks a buffer that has been filled in by the kernel, having gotten the memory
/// from a buffer ring, and returned to userland via a cqe entry.
pub struct BufX<G: Group> {
    // TODO remove bgroup: BufRingRc,
    bgroup: G,
    len: usize,
    bid: Bid,
}

// TODO remove deadcode
#[allow(dead_code)]
impl<G> BufX<G>
where
    G: Group,
{
    pub(crate) fn new(bgroup: G, bid: Bid, len: usize) -> Self {
        // len will already have been checked against the buf_capacity
        // so it is guaranteed that len <= bgroup.buf_capacity.

        Self { bgroup, len, bid }
    }

    /// Return the number of bytes initialized.
    ///
    /// This value initially came from the kernel, as reported in the cqe. This value may have been
    /// modified with a call to the IoBufMut::set_init method.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as _
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
    pub fn as_slice(&'static self) -> &'static [u8] {
        let p = self.bgroup.stable_ptr(self.bid);
        unsafe { std::slice::from_raw_parts(p, self.len) }
    }

    /// Return a mutable byte slice reference.
    #[inline]
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        let p = self.bgroup.stable_mut_ptr(self.bid);
        unsafe { std::slice::from_raw_parts_mut(p, self.len) }
    }

    // Future: provide access to the uninit space between len and cap if the buffer is being
    // repurposed before being dropped. The set_init below does that too.
}

impl<G> Drop for BufX<G>
where
    G: Group,
{
    fn drop(&mut self) {
        // Add the buffer back to the bgroup, for the kernel to reuse.
        unsafe { self.bgroup.dropping_bid(self.bid) };
    }
}

unsafe impl<G> crate::buf::IoBuf for BufX<G>
where
    G: Group + std::marker::Unpin + 'static,
{
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

unsafe impl<G> crate::buf::IoBufMut for BufX<G>
where
    G: Group + std::marker::Unpin + 'static,
{
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
