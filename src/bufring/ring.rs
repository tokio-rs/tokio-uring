//! Bindings for the io_uring interface's buf_ring feature.
//!
//! Whle a buf_ring is exhaused, new calls to read and that are or are not ready to read will fail
//! with the 105 error, "no buffers", while existing calls that were waiting to become ready to
//! read will not fail. Only when they data becomes ready to read will they fail, if the buffer
//! ring is still empty at that time.

use io_uring::sys::{__u64, io_uring_buf};
use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;
use std::sync::atomic::{self, AtomicU16};

use crate::bufgroup::{self, Bgid, Bid, BufX};

/// TODO
pub struct BufRing {
    bgid: Bgid,

    // Keep mask rather than ring size because mask is used often, ring size not.
    //ring_entries: u16, // Invariants: > 0, power of 2, max 2^15 (32768).
    ring_entries_mask: u16, // Invariant one less than ring_entries which is > 0, power of 2, max 2^15 (32768).

    buf_cnt: u16,   // Invariants: > 0, <= ring_entries.
    buf_len: usize, // Invariant: > 0.
    layout: std::alloc::Layout,
    ptr: *mut u8,     // Invariant: non zero.
    buf_ptr: *mut u8, // Invariant: non zero.
    local_tail: Cell<u16>,
    br_tail: *const AtomicU16,
    registered: Cell<bool>,

    // The two `possible` fields are a little suspect. They are an attempt at letting the caller
    // determine the sizing needs of their buf_ring through some experimentation. After running
    // tests, the caller can report what the possible min value was, if it looks too close to zero,
    // the model may warrant adjustment.
    //
    // We don't really know how deep the uring
    // interface has gone into the buffer ring because we never see its head value and it can be
    // taking buffers from the ring, in-flight, while we add buffers back to the ring. All we know is
    // when we are asked for buffer, given a bid read from a cqe, we can assume that call is made once per cqe for
    // a buffer grouped operation, and we know the add is done once per buffer being added back to
    // the buffer ring.  If the model changes and there are other reasons for the buffer to be
    // provided, the `posible_cur` will be lowered too much and will definitely be inaccurate.
    possible_min: Cell<u16>,
    possible_cur: Cell<u16>,
    // Future: work out how this BufRing could be used for other uring drivers, not just the one found at CURRENT.
    // May involve building with an optional driver.
    #[cfg(any(debug, feature = "cautious"))]
    debug_bitmap: RefCell<std::vec::Vec<u8>>,
}

#[derive(Debug, Copy, Clone)]
/// TODO
/// Build the arguments to call build() that returns a BufRing.
/// Refer to the methods descriptions for details.
pub struct Builder {
    bgid: Bgid,
    ring_entries: u16,
    buf_cnt: u16,
    buf_len: usize,
    buf_align: usize,
    ring_pad: usize,
    bufend_align: usize,

    skip_register: bool,
}

impl Builder {
    /// TODO
    /// Create a new Builder with the given buffer group ID and defaults.
    ///
    /// The buffer group ID, `bgid`, is the id the kernel uses to identify the buffer group to use
    /// for a given read operation that has been placed into an sqe.
    ///
    /// The caller is responsible for picking a bgid that does not comflict with other buffer
    /// groups that have been registered with the same uring interface.
    ///
    pub fn new(bgid: Bgid) -> Builder {
        Builder {
            bgid,
            ring_entries: 128,
            buf_cnt: 0,
            buf_len: 4096,
            buf_align: 0,
            ring_pad: 0,
            bufend_align: 0,
            skip_register: false,
        }
    }

    /// The number of ring entries to create for the buffer ring.
    ///
    /// The number will be made a power of 2, and will be the maximum of the ring_entries setting
    /// and the buf_cnt setting. The interface will enforce a maximum of 2^15 (32768) so it can do
    /// rollover calculation.
    pub fn ring_entries(mut self, ring_entries: u16) -> Builder {
        self.ring_entries = ring_entries;
        self
    }

    /// The number of buffers to allocate. If left zero, the ring_entries value will be used.
    pub fn buf_cnt(mut self, buf_cnt: u16) -> Builder {
        self.buf_cnt = buf_cnt;
        self
    }

    /// The length to be preallocated for each buffer.
    pub fn buf_len(mut self, buf_len: usize) -> Builder {
        self.buf_len = buf_len;
        self
    }

    /// The alignment of the first buffer allocated.
    ///
    /// Can be ommitted if not considered important.
    /// TODO ommitted?
    pub fn buf_align(mut self, buf_align: usize) -> Builder {
        self.buf_align = buf_align;
        self
    }

    /// Pad to place after ring to ensure separation between rings and first buffer.
    pub fn ring_pad(mut self, ring_pad: usize) -> Builder {
        self.ring_pad = ring_pad;
        self
    }

    /// The alignment of the end of the buffer allocated. To keep other things out of a cache line
    /// or out of a page, if that's desired.
    pub fn bufend_align(mut self, bufend_align: usize) -> Builder {
        self.bufend_align = bufend_align;
        self
    }

    /// Skip automatic registration. The caller can manually invoke the buf_ring.register()
    /// function later. Regardless, the unregister() method will be called automatically when the
    /// BufRing goes out of scope if the caller hadn't manually called buf_ring.unregister()
    /// already.
    pub fn skip_auto_register(mut self) -> Builder {
        self.skip_register = true;
        self
    }

    /// TODO
    /// Return a BufRing, having computed the layout for the single aligned allocation
    /// of both the buffer ring elements and the buffers themselves.
    ///
    /// If auto_register was left enabled, register the BufRing with the uring interface via the
    /// CURRENT driver reference.
    ///
    ///
    pub fn build(&self) -> io::Result<BufRingRc> {
        let mut b: Builder = *self;

        // Two cases where both buf_cnt and ring_entries are set to the max of the two.
        if b.buf_cnt == 0 || b.ring_entries < b.buf_cnt {
            let max = std::cmp::max(b.ring_entries, b.buf_cnt);
            b.buf_cnt = max;
            b.ring_entries = max;
        }

        // Don't allow the next_power_of_two calculation to be done if already larger than 2^15
        // because 2^16 reads back as 0 in a u16. And the interface doesn't allow for ring_entries
        // larger than 2^15 anyway, so this is a good place to catch it. Here we return a unique
        // error that is more descriptive than the InvalidArg that would come from the interface.
        if b.ring_entries > (1 << 15) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "ring_entries exceeded 32768",
            ));
        }

        // Requirement of the interface is the ring entries is a power of two, making its and our
        // mask calculation trivial.
        b.ring_entries = b.ring_entries.next_power_of_two();

        let inner = BufRing::new(
            b.bgid,
            b.ring_entries,
            b.buf_cnt,
            b.buf_len,
            b.buf_align,
            b.ring_pad,
            b.bufend_align,
            !b.skip_register,
        )?;
        Ok(BufRingRc::new(inner))
    }
}

impl BufRing {
    #[allow(clippy::too_many_arguments)]
    fn new(
        bgid: Bgid,
        ring_entries: u16,
        buf_cnt: u16,
        buf_len: usize,
        buf_align: usize,
        ring_pad: usize,
        bufend_align: usize,
        auto_register: bool,
    ) -> io::Result<BufRing> {
        #[allow(non_upper_case_globals)]
        const trace: bool = false;

        // Check that none of the important args are zero and the ring_entries is at least large
        // enough to hold all the buffers and that ring_entries is a power of 2.

        if (buf_cnt == 0)
            || (buf_cnt > ring_entries)
            || (buf_len == 0)
            || ((ring_entries & (ring_entries - 1)) != 0)
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }

        // entry_size is 64 bytes.
        let entry_size = std::mem::size_of::<io_uring_buf>() as usize;
        let mut ring_size = entry_size * (ring_entries as usize);
        if trace {
            println!(
                "{}:{}: entry_size {} * ring_entries {} = ring_size {} {:#x}",
                file!(),
                line!(),
                entry_size,
                ring_entries,
                ring_size,
                ring_size,
            );
        }

        ring_size += ring_pad;

        if trace {
            println!(
                "{}:{}: after +ring_pad {} ring_size {} {:#x}",
                file!(),
                line!(),
                ring_pad,
                ring_size,
                ring_size,
            );
        }

        if buf_align > 0 {
            let buf_align = buf_align.next_power_of_two();
            ring_size = (ring_size + (buf_align - 1)) & !(buf_align - 1);
            if trace {
                println!(
                    "{}:{}: after buf_align  ring_size {} {:#x}",
                    file!(),
                    line!(),
                    ring_size,
                    ring_size,
                );
            }
        }
        let buf_size = buf_len * (buf_cnt as usize);
        let mut tot_size: usize = ring_size + buf_size;
        if trace {
            println!(
                "{}:{}: ring_size {} {:#x} + buf_size {} {:#x} = tot_size {} {:#x}",
                file!(),
                line!(),
                ring_size,
                ring_size,
                buf_size,
                buf_size,
                tot_size,
                tot_size
            );
        }
        if bufend_align > 0 {
            // for example, if bufend_align is 4096, would make total size a multiple of pages
            let bufend_align = bufend_align.next_power_of_two();
            tot_size = (tot_size + (bufend_align - 1)) & !(bufend_align - 1);
            if trace {
                println!(
                    "{}:{}: after bufend_align tot_size {} {:#x}",
                    file!(),
                    line!(),
                    tot_size,
                    tot_size,
                );
            }
        }
        let align: usize = 4096; // alignment should be the page size
        let align = align.next_power_of_two();
        let layout = std::alloc::Layout::from_size_align(tot_size, align).unwrap();

        let ptr: *mut u8 = unsafe { std::alloc::alloc_zeroed(layout) };
        // Buffers starts after the ring_size.
        let buf_ptr: *mut u8 = unsafe { ptr.add(ring_size) };
        if trace {
            println!(
                "{}:{}:     ptr {} {:#x}, layout: size {} align {}",
                file!(),
                line!(),
                ptr as u64,
                ptr as u64,
                layout.size(),
                layout.align()
            );
            println!(
                "{}:{}: buf_ptr {} {:#x}",
                file!(),
                line!(),
                buf_ptr as u64,
                buf_ptr as u64,
            );
        }

        // TODO
        let br_tail = unsafe { ptr.offset(14) } as *const AtomicU16;

        let ring_entries_mask = ring_entries - 1;
        assert!((ring_entries & ring_entries_mask) == 0);

        let buf_ring = BufRing {
            bgid,
            ring_entries_mask,
            buf_cnt,
            buf_len,
            layout,
            ptr,
            buf_ptr,
            local_tail: Cell::new(0),
            br_tail,
            registered: Cell::new(false),

            possible_min: Cell::new(buf_cnt),
            possible_cur: Cell::new(0), // will be incremented when buffers added

            #[cfg(any(debug, feature = "cautious"))]
            debug_bitmap: RefCell::new(std::vec![0; ((buf_cnt+7)/8) as usize]),
        };

        // Question comes up: where should the initial buffers be added to the ring?
        // Here when the ring is created, even before it is registered potentially?
        // Or in the register, but before the register? Or after?
        //
        // For now, we are adding the buffers after the registration. It involves not extra system
        // calls and it is more like when a buffer is going out of scope and is being added back to
        // the ring.
        //
        //
        //    for bid in 0..buf_cnt {
        //        buf_ring.buf_ring_add(buf_ring.stable_ptr(bid) as _, buf_len, bid);
        //    }
        //    buf_ring.buf_ring_sync();

        // The default is to register the buffer ring right here. There is usually no reason the
        // caller should want to register it some time later. This model of a buffer ring only
        // allows it to be associated with one uring interface anyway.
        //
        // Perhaps the caller wants to allocate the buffer ring before the CURRENT driver is in
        // place - that would be a reason to delay the register call until later.

        if auto_register {
            buf_ring.register()?;
        }
        Ok(buf_ring)
    }

    /// Register the buffer ring with the uring interface.
    /// Normally this is done automatically when building a BufRing.
    ///
    /// Warning: requires the CURRENT driver is already in place or will panic.
    fn register(&self) -> io::Result<()> {
        // println!("{}:{}: register", file!(), line!());
        let bgid = self.bgid;

        // TODO move to separate public function so other buf_ring implementations
        // can register, and unregister, the same way.
        let res = crate::driver::CURRENT.with(|x| {
            RefCell::borrow_mut(x).uring.submitter().register_buf_ring(
                self.ptr as _,
                self.ring_entries(),
                bgid,
            )?;
            Ok::<(), io::Error>(())
        });
        // println!("{}:{}: res {:?}", file!(), line!(), res);

        if let Err(e) = res {
            match e.raw_os_error() {
                Some(22) => {
                    // using buf_ring requires kernel 5.19 or greater.
                    // TODO turn these eprintln into new, more expressive error being returned.
                    eprintln!(
                        "buf_ring.register returned {}, most likely indicating this kernel is not 5.19+",
                        e,
                        );
                }
                Some(17) => {
                    // Registering a duplicate bgid is not allowed. There is an `unregister`
                    // operations that can remove the first, but care must be taken that there
                    // are no outstanding operations that will still return a buffer from that
                    // one.
                    eprintln!(
                        "buf_ring.register returned `{}`, indicating the attempted buffer group id {} was already registered",
                        e,
                        bgid,
                        );
                }
                _ => {
                    eprintln!("buf_ring.register returned `{}` for group id {}", e, bgid);
                }
            }
            return Err(e);
        };

        self.registered.set(true);

        // Add the buffers after the registration. Really seems it could be done earlier too.

        for bid in 0..self.buf_cnt {
            self.buf_ring_add(self.stable_ptr_i(bid) as _, self.buf_len, bid);
        }
        self.buf_ring_sync();

        res
    }

    /// Unregister the buffer ring from the io_uring.
    /// Normally this is done automatically when the BufRing goes out of scope.
    ///
    /// Warning: requires the CURRENT driver is already in place or will panic.
    fn unregister(&self) -> io::Result<()> {
        if !self.registered.get() {
            return Ok(());
        }
        self.registered.set(false);

        let bgid = self.bgid;

        crate::driver::CURRENT.with(|x| {
            RefCell::borrow_mut(x)
                .uring
                .submitter()
                .unregister_buf_ring(bgid)
        })
    }

    /// Returns the buffer group id.
    #[inline]
    fn bgid(&self) -> Bgid {
        self.bgid
    }

    fn get_buf<G>(&self, res: u32, flags: u32, buf_ring: G) -> io::Result<BufX<G>>
    where
        G: bufgroup::Group,
    {
        // This fn does the odd thing of have self as the BufRing and take an argument that is the
        // same BufRing but wrapped in Rc<RefCell<_>> so the wrapped buf_ring can be passed to the
        // outgoing BufX.

        assert!(flags & io_uring::sys::IORING_CQE_F_BUFFER != 0);
        let bid = (flags >> 16) as u16;

        let len = res as usize;

        /* pretty interesting
        let flags = flags & !io_uring::sys::IORING_CQE_F_BUFFER; // for tracing flags
        println!(
            "{}:{}: get_buf res({res})=len({len}) flags({:#x})->bid({bid})\n\n",
            file!(),
            line!(),
            flags
        );
        */

        assert!(len <= self.buf_len);

        #[cfg(any(debug, feature = "cautious"))]
        {
            let mut debug_bitmap = self.debug_bitmap.borrow_mut();
            let m = 1 << (bid % 8);
            assert!(debug_bitmap[(bid / 8) as usize] & m == m);
            debug_bitmap[(bid / 8) as usize] &= !m;
        }

        self.possible_cur.set(self.possible_cur.get() - 1);
        self.possible_min.set(std::cmp::min(
            self.possible_min.get(),
            self.possible_cur.get(),
        ));
        /*
        println!(
            "{}:{}: get_buf cur {}, min {}",
            file!(),
            line!(),
            self.possible_cur.get(),
            self.possible_min.get(),
        );
        */

        Ok(BufX::new(buf_ring, bid, len))
    }

    // Safety: dropping a duplicate bid is likely to cause undefined behavior
    // as the kernel uses the same buffer for different data concurrently.
    unsafe fn dropping_bid_i(&self, bid: Bid) {
        self.buf_ring_add(self.stable_ptr_i(bid) as _, self.buf_len, bid);
        self.buf_ring_sync();
    }

    #[inline]
    fn buf_capacity_i(&self) -> usize {
        self.buf_len as _
    }

    #[inline]
    fn stable_ptr_i(&self, bid: Bid) -> *const u8 {
        assert!(bid < self.buf_cnt);
        let offset: usize = self.buf_len * (bid as usize);
        unsafe { self.buf_ptr.add(offset) }
    }

    // # Safety
    //
    // This should only be called by an owned or &mut object.
    #[inline]
    unsafe fn stable_mut_ptr_i(&self, bid: Bid) -> *mut u8 {
        assert!(bid < self.buf_cnt);
        let offset: usize = self.buf_len * (bid as usize);
        //unsafe { self.buf_ptr.add(offset) }
        self.buf_ptr.add(offset)
    }

    #[inline]
    fn ring_entries(&self) -> u16 {
        self.ring_entries_mask + 1
    }

    #[inline]
    fn mask(&self) -> u16 {
        self.ring_entries_mask
    }

    // Assign 'buf' with the addr/len/buffer ID supplied
    fn buf_ring_add(&self, addr: __u64, len: usize, bid: Bid) {
        assert!(bid < self.buf_cnt);
        assert!(len == self.buf_len);
        // TODO assert addr matches bid.

        let buf = self.ptr as *mut io_uring_buf;
        let old_tail = self.local_tail.get();
        let new_tail = old_tail + 1;
        self.local_tail.set(new_tail);
        let ring_idx = old_tail & self.mask();

        let buf = unsafe { &mut *buf.add(ring_idx as usize) };

        buf.addr = addr;
        buf.len = len as _;
        buf.bid = bid;
        /* most interesting
        println!(
            "{}:{}: buf_ring_add {:?} old_tail {old_tail} new_tail {new_tail} ring_idx {ring_idx}",
            file!(),
            line!(),
            buf
        );
        */
        self.possible_cur.set(self.possible_cur.get() + 1);

        #[cfg(any(debug, feature = "cautious"))]
        {
            let mut debug_bitmap = self.debug_bitmap.borrow_mut();
            let m = 1 << (bid % 8);
            assert!(debug_bitmap[(bid / 8) as usize] & m == 0);
            debug_bitmap[(bid / 8) as usize] |= m;
        }
    }

    // Make 'count' new buffers visible to the kernel. Called after
    // io_uring_buf_ring_add() has been called 'count' times to fill in new
    // buffers.
    #[inline]
    fn buf_ring_sync(&self) {
        /*
        println!(
            "{}:{}: sync stores tail {}",
            file!(),
            line!(),
            self.local_tail.get()
        );
        */
        unsafe {
            //atomic::fence(atomic::Ordering::SeqCst);
            (*self.br_tail).store(self.local_tail.get(), atomic::Ordering::Release);
        }

        // TODO figure out if this is needed, io_uring_smp_store_release(&br.tail, local_tail);
    }

    /// TODO
    /// Return the possible_min buffer pool size.
    fn possible_min(&self) -> u16 {
        self.possible_min.get()
    }
    /// TODO
    /// Return the possible_min buffer pool size and reset
    /// to allow fresh counting going forward.
    fn possible_min_and_reset(&self) -> u16 {
        let res = self.possible_min.get();
        self.possible_min.set(self.buf_cnt);
        res
    }
}

impl Drop for BufRing {
    fn drop(&mut self) {
        if self.registered.get() {
            _ = self.unregister();
        }
        unsafe { std::alloc::dealloc(self.ptr, self.layout) };
    }
}

// TODO rename so BufRing becomes BufRingI
// and then BufRingRc can become FixedBufRing.
/// TODO
#[derive(Clone)]
pub struct BufRingRc {
    //rc: Rc<RefCell<BufRing>>,
    rc: Rc<BufRing>,
}
impl BufRingRc {
    /// TODO
    pub fn new(buf_ring: BufRing) -> Self {
        BufRingRc {
            //rc: Rc::new(RefCell::new(buf_ring)),
            rc: Rc::new(buf_ring),
        }
    }
    /// TODO
    pub fn register(&self) -> io::Result<()> {
        //self.rc.borrow().register()
        self.rc.register()
    }
    /// TODO
    pub fn unregister(&self) -> io::Result<()> {
        //self.rc.borrow().unregister()
        self.rc.unregister()
    }
}

impl BufRingRc {
    /// TODO
    /// Return the possible_min buffer pool size.
    pub fn possible_min(&self) -> u16 {
        //self.rc.borrow().possible_min()
        self.rc.possible_min()
    }
    /// Return the possible_min buffer pool size and reset
    /// to allow fresh counting going forward.
    #[allow(dead_code)]
    fn possible_min_and_reset(&self) -> u16 {
        self.rc.possible_min_and_reset()
    }
}

impl bufgroup::Group for BufRingRc {
    // Return the buffer group ID.
    fn bgid(&self) -> Bgid {
        //self.rc.borrow().bgid()
        self.rc.bgid()
    }

    fn buf_capacity(&self, _: Bid) -> usize {
        //self.rc.borrow().buf_capacity_i()
        self.rc.buf_capacity_i()
    }

    fn stable_ptr(&self, bid: Bid) -> *const u8 {
        //self.rc.borrow().stable_ptr_i(bid)
        self.rc.stable_ptr_i(bid)
    }

    fn stable_mut_ptr(&mut self, bid: Bid) -> *mut u8 {
        //self.rc.borrow_mut().stable_mut_ptr_i(bid)
        // # Safety
        // self is &mut, and that's the safety guarantee stable_mut_ptr_i asks for.
        //unsafe{ self.rc.borrow().stable_mut_ptr_i(bid) }
        unsafe { self.rc.stable_mut_ptr_i(bid) }
    }

    // Safety: dropping_bid should only be called by the buffer's drop function
    // because once called, the buffer may be given back to the kernel for reuse.
    unsafe fn dropping_bid(&self, bid: Bid) {
        //self.rc.borrow().dropping_bid_i(bid);
        self.rc.dropping_bid_i(bid);
    }

    // This one isn't needed by BufX, but is by the operations.
    fn get_buf<G>(&self, res: u32, flags: u32, buf_ring: G) -> io::Result<BufX<G>>
    where
        G: bufgroup::Group,
    {
        //self.rc.borrow().get_buf(res, flags, buf_ring)
        self.rc.get_buf(res, flags, buf_ring)
    }

    /* not quite
     *
    fn get_buf2<G>(self, res: u32, flags: u32) -> io::Result<BufX<G>>
    where
        G: bufgroup::Group,
    {
        let buf_ring = self.clone();
        //self.rc.borrow().get_buf(res, flags, buf_ring)
        self.rc.get_buf(res, flags, buf_ring)
    }
    */
}
