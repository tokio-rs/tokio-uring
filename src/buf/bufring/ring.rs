//! Module for the io_uring device's buf_ring feature.

// Developer's note about io_uring return codes when a buf_ring is used:
//
// While a buf_ring pool is exhaused, new calls to read that are, or are not, ready to read will
// fail with the 105 error, "no buffers", while existing calls that were waiting to become ready to
// read will not fail. Only when the data becomes ready to read will they fail, if the buffer ring
// is still empty at that time. This makes sense when thinking about it from how the kernel
// implements the start of a read command; it can be confusing when first working with these
// commands from the userland perspective.

// While the file! calls yield the clippy false positive.
#![allow(clippy::print_literal)]

use io_uring::types;
use std::cell::Cell;
use std::io;
use std::rc::Rc;
use std::sync::atomic::{self, AtomicU16};

use super::super::bufgroup::{Bgid, Bid, BufX};
use crate::runtime::CONTEXT;

/// A `BufRing` represents the ring and the buffers used with the kernel's io_uring buf_ring
/// feature.
///
/// In this implementation, it is both the ring of buffer entries and the actual buffer
/// allocations.
///
/// A BufRing is created through the [`Builder`] and can be registered automatically by the
/// builder's `build` step or at a later time by the user. Registration involves informing the
/// kernel of the ring's dimensions and its identifier (its buffer group id, which goes by the name
/// `bgid`).
///
/// Multiple buf_rings, here multiple BufRings, can be created and registered. BufRings are
/// reference counted to ensure their memory is live while their BufX buffers are live. When a BufX
/// buffer is dropped, it releases itself back to the BufRing from which it came allowing it to be
/// reused by the kernel.
///
/// It is perhaps worth pointing out that it is the ring itself that is registered with the kernel,
/// not the buffers per se. While a given buf_ring cannot have it size changed dynamically, the
/// buffers that are pushed to the ring by userland, and later potentially re-pushed in the ring,
/// can change. The buffers can be of different sizes and they could come from different allocation
/// blocks. This implementation does not provide that flexibility. Each BufRing comes with its own
/// equal length buffer allocation. And when a BufRing buffer, a BufX, is dropped, its id is pushed
/// back to the ring.
///
/// This is the one and only `Provided Buffers` implementation in `tokio_uring` at the moment and
/// in this version, is a purely concrete type, with a concrete BufX type for buffers that are
/// returned by operations like `recv_provbuf` to the userland application.
///
/// Aside from the register and unregister steps, there are no syscalls used to pass buffers to the
/// kernel. The ring contains a tail memory address that this userland type updates as buffers are
/// added to the ring and which the kernel reads when it needs to pull a buffer from the ring. The
/// kernel does not have a head pointer address that it updates for the userland. The userland
/// (this type), is expected to avoid overwriting the head of the circular ring by keeping track of
/// how many buffers were added to the ring and how many have been returned through the CQE
/// mechanism. This particular implementation does not track the count because all buffers are
/// allocated at the beginning, by the builder, and only its own buffers that came back via a CQE
/// are ever added back to the ring, so it should be impossible to overflow the ring.
#[derive(Clone, Debug)]
pub struct BufRing {
    // RawBufRing uses cell for fields where necessary.
    raw: Rc<RawBufRing>,
}

// Methods for the user.

impl BufRing {
    /// Registers the buf_ring manually.
    ///
    /// This is not normally called because the builder defaults to registering the ring when it
    /// builds the ring. This is provided for the case where a BufRing is being built before the
    /// tokio_uring runtime has started.
    pub fn register(&self) -> io::Result<()> {
        self.raw.register()
    }

    /// Unregisters the buf_ring manually.
    ///
    /// This not normally called because the drop mechanism will unregister the ring if it had not
    /// already been unregistered.
    ///
    /// This function makes it possible to unregister a ring while some of its BufX buffers may
    /// still be live. This does not result in UB. It just means the io_uring device will not have
    /// this buf_ring pool to draw from. Put another way, the step of unregistering the ring does
    /// not deallocate the buffers.
    pub fn unregister(&self) -> io::Result<()> {
        self.raw.unregister()
    }

    /// (Very experimental and should probably be behind a cfg option.)
    ///
    /// Returns the lowest level buffer pool size that has been observed. It cannot be accurate
    /// because it cannot take into account in-flight operations that may draw from the pool.
    ///
    /// This might be useful when running the system and trying to decide if the pool was sized
    /// correctly. Maintaining this value does come with a small overhead.
    #[allow(dead_code)]
    pub fn possible_min(&self) -> u16 {
        self.raw.possible_min()
    }

    /// (Very experimental and should probably be behind a cfg option.)
    ///
    /// Like `possible_min` but also resets the metric to the total number of buffers that had been
    /// allocated.
    #[allow(dead_code)]
    pub fn possible_min_and_reset(&self) -> u16 {
        self.raw.possible_min_and_reset()
    }
}

// Methods the BufX needs.

impl BufRing {
    pub(crate) fn buf_capacity(&self, _: Bid) -> usize {
        self.raw.buf_capacity_i()
    }

    pub(crate) fn stable_ptr(&self, bid: Bid) -> *const u8 {
        // Will panic if bid is out of range.
        self.raw.stable_ptr_i(bid)
    }

    pub(crate) fn stable_mut_ptr(&mut self, bid: Bid) -> *mut u8 {
        // Safety: self is &mut, we're good.
        unsafe { self.raw.stable_mut_ptr_i(bid) }
    }

    // # Safety
    //
    // `dropping_bid` should only be called by the buffer's drop function because once called, the
    // buffer may be given back to the kernel for reuse.
    pub(crate) unsafe fn dropping_bid(&self, bid: Bid) {
        self.raw.dropping_bid_i(bid);
    }
}

// Methods the io operations need.

impl BufRing {
    pub(crate) fn bgid(&self) -> Bgid {
        self.raw.bgid()
    }

    // # Safety
    //
    // The res and flags values are used to lookup a buffer and set its initialized length.
    // The caller is responsible for these being correct. This is expected to be called
    // when these two values are received from the kernel via a CQE and we rely on the kernel to
    // give us correct information.
    pub(crate) unsafe fn get_buf(&self, res: u32, flags: u32) -> io::Result<Option<BufX>> {
        let bid = match io_uring::cqueue::buffer_select(flags) {
            Some(bid) => bid,
            None => {
                // Have seen res == 0, flags == 4 with a TCP socket. res == 0 we take to mean the
                // socket is empty so return None to show there is no buffer returned, which should
                // be interpreted to mean there is no more data to read from this file or socket.
                if res == 0 {
                    return Ok(None);
                }

                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "BufRing::get_buf failed as the buffer bit, IORING_CQE_F_BUFFER, was missing from flags, res = {}, flags = {}",
                        res, flags)
                ));
            }
        };

        let len = res as usize;

        /*
        let flags = flags & !io_uring::sys::IORING_CQE_F_BUFFER; // for tracing flags
        println!(
            "{}:{}: get_buf res({res})=len({len}) flags({:#x})->bid({bid})\n\n",
            file!(),
            line!(),
            flags
        );
        */

        assert!(len <= self.raw.buf_len);

        // TODO maybe later
        // #[cfg(any(debug, feature = "cautious"))]
        // {
        //     let mut debug_bitmap = self.debug_bitmap.borrow_mut();
        //     let m = 1 << (bid % 8);
        //     assert!(debug_bitmap[(bid / 8) as usize] & m == m);
        //     debug_bitmap[(bid / 8) as usize] &= !m;
        // }

        self.raw.metric_getting_another();
        /*
        println!(
            "{}:{}: get_buf cur {}, min {}",
            file!(),
            line!(),
            self.possible_cur.get(),
            self.possible_min.get(),
        );
        */

        // Safety: the len provided to BufX::new is given to us from the kernel.
        Ok(Some(unsafe { BufX::new(self.clone(), bid, len) }))
    }
}

#[derive(Debug, Copy, Clone)]
/// Build the arguments to call build() that returns a [`BufRing`].
///
/// Refer to the methods descriptions for details.
#[allow(dead_code)]
pub struct Builder {
    page_size: usize,
    bgid: Bgid,
    ring_entries: u16,
    buf_cnt: u16,
    buf_len: usize,
    buf_align: usize,
    ring_pad: usize,
    bufend_align: usize,

    skip_register: bool,
}

#[allow(dead_code)]
impl Builder {
    /// Create a new Builder with the given buffer group ID and defaults.
    ///
    /// The buffer group ID, `bgid`, is the id the kernel's io_uring device uses to identify the
    /// provided buffer pool to use by operations that are posted to the device.
    ///
    /// The user is responsible for picking a bgid that does not conflict with other buffer groups
    /// that have been registered with the same uring interface.
    pub fn new(bgid: Bgid) -> Builder {
        Builder {
            page_size: 4096,
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

    /// The page size of the kernel. Defaults to 4096.
    ///
    /// The io_uring device requires the BufRing is allocated on the start of a page, i.e. with a
    /// page size alignment.
    ///
    /// The caller should determine the page size, and may want to cache the info if multiple buf
    /// rings are to be created. Crates are available to get this information or the user may want
    /// to call the libc sysconf directly:
    ///
    ///     use libc::{_SC_PAGESIZE, sysconf};
    ///     let page_size: usize = unsafe { sysconf(_SC_PAGESIZE) as usize };
    pub fn page_size(mut self, page_size: usize) -> Builder {
        self.page_size = page_size;
        self
    }

    /// The number of ring entries to create for the buffer ring.
    ///
    /// This defaults to 128 or the `buf_cnt`, whichever is larger.
    ///
    /// The number will be made a power of 2, and will be the maximum of the ring_entries setting
    /// and the buf_cnt setting. The interface will enforce a maximum of 2^15 (32768) so it can do
    /// rollover calculation.
    ///
    /// Each ring entry is 16 bytes.
    pub fn ring_entries(mut self, ring_entries: u16) -> Builder {
        self.ring_entries = ring_entries;
        self
    }

    /// The number of buffers to allocate. If left zero, the ring_entries value will be used and
    /// that value defaults to 128.
    pub fn buf_cnt(mut self, buf_cnt: u16) -> Builder {
        self.buf_cnt = buf_cnt;
        self
    }

    /// The length of each allocated buffer. Defaults to 4096.
    ///
    /// Non-alignment values are possible and `buf_align` can be used to allocate each buffer on
    /// an alignment buffer, even if the buffer length is not desired to equal the alignment.
    pub fn buf_len(mut self, buf_len: usize) -> Builder {
        self.buf_len = buf_len;
        self
    }

    /// The alignment of the first buffer allocated.
    ///
    /// Generally not needed.
    ///
    /// The buffers are allocated right after the ring unless `ring_pad` is used and generally the
    /// buffers are allocated contiguous to one another unless the `buf_len` is set to something
    /// different.
    pub fn buf_align(mut self, buf_align: usize) -> Builder {
        self.buf_align = buf_align;
        self
    }

    /// Pad to place after ring to ensure separation between rings and first buffer.
    ///
    /// Generally not needed but may be useful if the ring's end and the buffers' start are to have
    /// some separation, perhaps for cacheline reasons.
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
    pub fn skip_auto_register(mut self, skip: bool) -> Builder {
        self.skip_register = skip;
        self
    }

    /// Return a BufRing, having computed the layout for the single aligned allocation
    /// of both the buffer ring elements and the buffers themselves.
    ///
    /// If auto_register was left enabled, register the BufRing with the driver.
    pub fn build(&self) -> io::Result<BufRing> {
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

        Ok(BufRing {
            raw: Rc::new(RawBufRing::new(NewArgs {
                page_size: b.page_size,
                bgid: b.bgid,
                ring_entries: b.ring_entries,
                buf_cnt: b.buf_cnt,
                buf_len: b.buf_len,
                buf_align: b.buf_align,
                ring_pad: b.ring_pad,
                bufend_align: b.bufend_align,
                auto_register: !b.skip_register,
            })?),
        })
    }
}

// Trivial helper struct for this module.
struct NewArgs {
    page_size: usize,
    bgid: Bgid,
    ring_entries: u16,
    buf_cnt: u16,
    buf_len: usize,
    buf_align: usize,
    ring_pad: usize,
    bufend_align: usize,
    auto_register: bool,
}

#[derive(Debug)]
struct RawBufRing {
    bgid: Bgid,

    // Keep mask rather than ring size because mask is used often, ring size not.
    //ring_entries: u16, // Invariants: > 0, power of 2, max 2^15 (32768).
    ring_entries_mask: u16, // Invariant one less than ring_entries which is > 0, power of 2, max 2^15 (32768).

    buf_cnt: u16,   // Invariants: > 0, <= ring_entries.
    buf_len: usize, // Invariant: > 0.
    layout: std::alloc::Layout,
    ring_addr: *const types::BufRingEntry, // Invariant: constant.
    buffers_addr: *mut u8,                 // Invariant: constant.
    local_tail: Cell<u16>,
    tail_addr: *const AtomicU16,
    registered: Cell<bool>,

    // The first `possible` field is a best effort at tracking the current buffer pool usage and
    // from that, tracking the lowest level that has been reached. The two are an attempt at
    // letting the user check the sizing needs of their buf_ring pool.
    //
    // We don't really know how deep the uring device has gone into the pool because we never see
    // its head value and it can be taking buffers from the ring, in-flight, while we add buffers
    // back to the ring. All we know is when a CQE arrives and a buffer lookup is performed, a
    // buffer has already been taken from the pool, and when the buffer is dropped, we add it back
    // to the the ring and it is about to be considered part of the pool again.
    possible_cur: Cell<u16>,
    possible_min: Cell<u16>,
    //
    // TODO maybe later
    // #[cfg(any(debug, feature = "cautious"))]
    // debug_bitmap: RefCell<std::vec::Vec<u8>>,
}

impl RawBufRing {
    fn new(new_args: NewArgs) -> io::Result<RawBufRing> {
        #[allow(non_upper_case_globals)]
        const trace: bool = false;

        let NewArgs {
            page_size,
            bgid,
            ring_entries,
            buf_cnt,
            buf_len,
            buf_align,
            ring_pad,
            bufend_align,
            auto_register,
        } = new_args;

        // Check that none of the important args are zero and the ring_entries is at least large
        // enough to hold all the buffers and that ring_entries is a power of 2.

        if (buf_cnt == 0)
            || (buf_cnt > ring_entries)
            || (buf_len == 0)
            || ((ring_entries & (ring_entries - 1)) != 0)
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }

        // entry_size is 16 bytes.
        let entry_size = std::mem::size_of::<types::BufRingEntry>();
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
        assert!(ring_size != 0);
        assert!(buf_size != 0);
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

        let align: usize = page_size; // alignment must be at least the page size
        let align = align.next_power_of_two();
        let layout = std::alloc::Layout::from_size_align(tot_size, align).unwrap();

        assert!(layout.size() >= ring_size);
        // Safety: we are assured layout has nonzero size, we passed the assert just above.
        let ring_addr: *mut u8 = unsafe { std::alloc::alloc_zeroed(layout) };

        // Buffers starts after the ring_size.
        // Safety: are we assured the address and the offset are in bounds because the ring_addr is
        // the value we got from the alloc call, and the layout.size was shown to be at least as
        // large as the ring_size.
        let buffers_addr: *mut u8 = unsafe { ring_addr.add(ring_size) };
        if trace {
            println!(
                "{}:{}:     ring_addr {} {:#x}, layout: size {} align {}",
                file!(),
                line!(),
                ring_addr as u64,
                ring_addr as u64,
                layout.size(),
                layout.align()
            );
            println!(
                "{}:{}: buffers_addr {} {:#x}",
                file!(),
                line!(),
                buffers_addr as u64,
                buffers_addr as u64,
            );
        }

        let ring_addr: *const types::BufRingEntry = ring_addr as _;

        // Safety: the ring_addr passed into tail is the start of the ring. It is both the start of
        // the ring and the first entry in the ring.
        let tail_addr = unsafe { types::BufRingEntry::tail(ring_addr) } as *const AtomicU16;

        let ring_entries_mask = ring_entries - 1;
        assert!((ring_entries & ring_entries_mask) == 0);

        let buf_ring = RawBufRing {
            bgid,
            ring_entries_mask,
            buf_cnt,
            buf_len,
            layout,
            ring_addr,
            buffers_addr,
            local_tail: Cell::new(0),
            tail_addr,
            registered: Cell::new(false),
            possible_cur: Cell::new(0),
            possible_min: Cell::new(buf_cnt),
            //
            // TODO maybe later
            // #[cfg(any(debug, feature = "cautious"))]
            // debug_bitmap: RefCell::new(std::vec![0; ((buf_cnt+7)/8) as usize]),
        };

        // Question had come up: where should the initial buffers be added to the ring?
        // Here when the ring is created, even before it is registered potentially?
        // Or after registration?
        //
        // For this type, BufRing, we are adding the buffers to the ring as the last part of creating the BufRing,
        // even before registration is optionally performed.
        //
        // We've seen the registration to be successful, even when the ring starts off empty.

        // Add the buffers here where the ring is created.

        for bid in 0..buf_cnt {
            buf_ring.buf_ring_add(bid);
        }
        buf_ring.buf_ring_sync();

        // The default is to register the buffer ring right here. There is usually no reason the
        // caller should want to register it some time later.
        //
        // Perhaps the caller wants to allocate the buffer ring before the CONTEXT driver is in
        // place - that would be a reason to delay the register call until later.

        if auto_register {
            buf_ring.register()?;
        }
        Ok(buf_ring)
    }

    /// Register the buffer ring with the kernel.
    /// Normally this is done automatically when building a BufRing.
    ///
    /// This method must be called in the context of a `tokio-uring` runtime.
    /// The registration persists for the lifetime of the runtime, unless
    /// revoked by the [`unregister`] method. Dropping the
    /// instance this method has been called on does revoke
    /// the registration and deallocate the buffer space.
    ///
    /// [`unregister`]: Self::unregister
    ///
    /// # Errors
    ///
    /// If a `Provided Buffers` group with the same `bgid` is already registered, the function
    /// returns an error.
    fn register(&self) -> io::Result<()> {
        let bgid = self.bgid;
        //println!("{}:{}: register bgid {bgid}", file!(), line!());

        // Future: move to separate public function so other buf_ring implementations
        // can register, and unregister, the same way.

        let res = CONTEXT.with(|x| {
            x.handle()
                .as_ref()
                .expect("Not in a runtime context")
                .register_buf_ring(self.ring_addr as _, self.ring_entries(), bgid)
        });
        // println!("{}:{}: res {:?}", file!(), line!(), res);

        if let Err(e) = res {
            match e.raw_os_error() {
                Some(22) => {
                    // using buf_ring requires kernel 5.19 or greater.
                    // TODO turn these eprintln into new, more expressive error being returned.
                    // TODO what convention should we follow in this crate for adding information
                    // onto an error?
                    eprintln!(
                        "buf_ring.register returned {e}, most likely indicating this kernel is not 5.19+",
                        );
                }
                Some(17) => {
                    // Registering a duplicate bgid is not allowed. There is an `unregister`
                    // operations that can remove the first.
                    eprintln!(
                        "buf_ring.register returned `{e}`, indicating the attempted buffer group id {bgid} was already registered",
                        );
                }
                _ => {
                    eprintln!("buf_ring.register returned `{e}` for group id {bgid}");
                }
            }
            return Err(e);
        };

        self.registered.set(true);

        res
    }

    /// Unregister the buffer ring from the io_uring.
    /// Normally this is done automatically when the BufRing goes out of scope.
    ///
    /// Warning: requires the CONTEXT driver is already in place or will panic.
    fn unregister(&self) -> io::Result<()> {
        // If not registered, make this a no-op.
        if !self.registered.get() {
            return Ok(());
        }

        self.registered.set(false);

        let bgid = self.bgid;

        // If there is no context, bail out with an Ok(()) because the registration and
        // the entire io_uring is already done anyway.
        CONTEXT.with(|x| {
            x.handle()
                .as_ref()
                .map_or(Ok(()), |handle| handle.unregister_buf_ring(bgid))
        })
    }

    /// Returns the buffer group id.
    #[inline]
    fn bgid(&self) -> Bgid {
        self.bgid
    }

    fn metric_getting_another(&self) {
        self.possible_cur.set(self.possible_cur.get() - 1);
        self.possible_min.set(std::cmp::min(
            self.possible_min.get(),
            self.possible_cur.get(),
        ));
    }

    // # Safety
    //
    // Dropping a duplicate bid is likely to cause undefined behavior
    // as the kernel uses the same buffer for different data concurrently.
    unsafe fn dropping_bid_i(&self, bid: Bid) {
        self.buf_ring_add(bid);
        self.buf_ring_sync();
    }

    #[inline]
    fn buf_capacity_i(&self) -> usize {
        self.buf_len as _
    }

    #[inline]
    // # Panic
    //
    // This function will panic if given a bid that is not within the valid range 0..self.buf_cnt.
    fn stable_ptr_i(&self, bid: Bid) -> *const u8 {
        assert!(bid < self.buf_cnt);
        let offset: usize = self.buf_len * (bid as usize);
        // Safety: buffers_addr is an u8 pointer and was part of an allocation large enough to hold
        // buf_cnt number of buf_len buffers. buffers_addr, buf_cnt and buf_len are treated as
        // constants and bid was just asserted to be less than buf_cnt.
        unsafe { self.buffers_addr.add(offset) }
    }

    // # Safety
    //
    // This may only be called by an owned or &mut object.
    //
    // # Panic
    // This will panic if bid is out of range.
    #[inline]
    unsafe fn stable_mut_ptr_i(&self, bid: Bid) -> *mut u8 {
        assert!(bid < self.buf_cnt);
        let offset: usize = self.buf_len * (bid as usize);
        // Safety: buffers_addr is an u8 pointer and was part of an allocation large enough to hold
        // buf_cnt number of buf_len buffers. buffers_addr, buf_cnt and buf_len are treated as
        // constants and bid was just asserted to be less than buf_cnt.
        self.buffers_addr.add(offset)
    }

    #[inline]
    fn ring_entries(&self) -> u16 {
        self.ring_entries_mask + 1
    }

    #[inline]
    fn mask(&self) -> u16 {
        self.ring_entries_mask
    }

    // Writes to a ring entry and updates our local copy of the tail.
    //
    // Adds the buffer known by its buffer id to the buffer ring. The buffer's address and length
    // are known given its bid.
    //
    // This does not sync the new tail value. The caller should use `buf_ring_sync` for that.
    //
    // Panics if the bid is out of range.
    fn buf_ring_add(&self, bid: Bid) {
        // Compute address of current tail position, increment the local copy of the tail. Then
        // write the buffer's address, length and bid into the current tail entry.

        let cur_tail = self.local_tail.get();
        self.local_tail.set(cur_tail.wrapping_add(1));
        let ring_idx = cur_tail & self.mask();

        let ring_addr = self.ring_addr as *mut types::BufRingEntry;

        // Safety:
        //    1. the pointer address (ring_addr), is set and const at self creation time,
        //       and points to a block of memory at least as large as the number of ring_entries,
        //    2. the mask used to create ring_idx is one less than
        //       the number of ring_entries, and ring_entries was tested to be a power of two,
        //    So the address gotten by adding ring_idx entries to ring_addr is guaranteed to
        //       be a valid address of a ring entry.
        let entry = unsafe { &mut *ring_addr.add(ring_idx as usize) };

        entry.set_addr(self.stable_ptr_i(bid) as _);
        entry.set_len(self.buf_len as _);
        entry.set_bid(bid);

        // Update accounting.
        self.possible_cur.set(self.possible_cur.get() + 1);

        // TODO maybe later
        // #[cfg(any(debug, feature = "cautious"))]
        // {
        //     let mut debug_bitmap = self.debug_bitmap.borrow_mut();
        //     let m = 1 << (bid % 8);
        //     assert!(debug_bitmap[(bid / 8) as usize] & m == 0);
        //     debug_bitmap[(bid / 8) as usize] |= m;
        // }
    }

    // Make 'count' new buffers visible to the kernel. Called after
    // io_uring_buf_ring_add() has been called 'count' times to fill in new
    // buffers.
    #[inline]
    fn buf_ring_sync(&self) {
        // Safety: dereferencing this raw pointer is safe. The tail_addr was computed once at init
        // to refer to the tail address in the ring and is held const for self's lifetime.
        unsafe {
            (*self.tail_addr).store(self.local_tail.get(), atomic::Ordering::Release);
        }
        // The liburing code did io_uring_smp_store_release(&br.tail, local_tail);
    }

    // Return the possible_min buffer pool size.
    fn possible_min(&self) -> u16 {
        self.possible_min.get()
    }

    // Return the possible_min buffer pool size and reset to allow fresh counting going forward.
    fn possible_min_and_reset(&self) -> u16 {
        let res = self.possible_min.get();
        self.possible_min.set(self.buf_cnt);
        res
    }
}

impl Drop for RawBufRing {
    fn drop(&mut self) {
        if self.registered.get() {
            _ = self.unregister();
        }
        // Safety: the ptr and layout are treated as constant, and ptr (ring_addr) was assigned by
        // a call to std::alloc::alloc_zeroed using the same layout.
        unsafe { std::alloc::dealloc(self.ring_addr as *mut u8, self.layout) };
    }
}
