use std::{alloc::Layout, cell::RefCell, io, ptr::NonNull, rc::Rc};

use buddy_alloc::{buddy_alloc::BuddyAlloc, BuddyAllocParam};
use libc::iovec;

use crate::runtime::CONTEXT;

use super::{buffers::AllocatableBuffers, handle::AllocatedBuf, FixedBuf, FixedBuffers};

/// An allocateable I/O buffer heap pre-registered with the kernel.
///
/// `FixedBufAllocator` allows the application to manage a buffer heap
/// allocated in memory, that can be registered in the current `tokio-uring`
/// context using the [`register`] method. Unlike [`FixedBufRegistry`],
/// individual buffers are not retrieved by index; instead, an available
/// buffer matching a specified capacity can be retrieved with the [`allocate_fixed`]
/// method. This allows some flexibility in managing buffers without pre-selecting
/// the desired capacities required, at the expense of some bookeeping overhead.
///
/// A `FixedBufAllocator` value is a lightweight handle for a buffer heap.
/// Cloning of a `FixedBufAllocator` creates a new reference to
/// the same buffer heap.
///
/// The buffers of the collection are not deallocated until:
/// - all `FixedBufAllocator` references to the heap have been dropped;
/// - The `tokio-uring` [`Runtime`] the buffers are registered with
///   has been dropped.
///
/// [`register`]: Self::register
/// [`allocate_fixed`]: Self::allocate_fixed
/// [`Runtime`]: crate::Runtime
#[derive(Clone)]
pub struct FixedBufAllocator {
    inner: Rc<RefCell<Inner>>,
}

impl FixedBufAllocator {
    /// Creates a new buffer heap of the requested size.
    pub fn new(size: usize) -> io::Result<Self> {
        // the buddy allocator will use some of the space in the buffer for metadata
        // and so the layout.size() will be greater than allocator.available_bytes()
        let layout = Layout::from_size_align(size, 4096)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        let heap = unsafe {
            let data = std::alloc::alloc(layout) as *mut u8;
            NonNull::new(data).unwrap()
        };
        Ok(FixedBufAllocator {
            inner: Rc::new(RefCell::new(Inner::with_heap(heap, layout))),
        })
    }

    /// Available bytes in the allocator
    pub fn available_bytes(&self) -> usize {
        self.inner.borrow().allocator.available_bytes()
    }

    /// Registers the buffer heap with the kernel.
    ///
    /// This method must be called in the context of a `tokio-uring` runtime.
    /// The registration persists for the lifetime of the runtime, unless
    /// revoked by the [`unregister`] method. Dropping the
    /// `FixedBufAllocator` instance this method has been called on does not revoke
    /// the registration or deallocate the buffers.
    ///
    /// [`unregister`]: Self::unregister
    ///
    /// This call can be blocked in the kernel to complete any operations
    /// in-flight on the same `io-uring` instance. The application is
    /// recommended to register buffers before starting any I/O operations.
    ///
    /// # Errors
    ///
    /// If collection of buffers is currently registered in the context
    /// of the `tokio-uring` runtime this call is made in, the function returns
    /// an error.
    ///
    /// If the requested heap size is greater than the limit imposed on a single
    /// `io_uring` [`iovec`] (at present 1GiB), the function returns an error.
    ///
    /// If there are insufficient kernel resources are available, or the caller had a
    /// non-zero [`RLIMIT_MEMLOCK`] soft resource limit, but tried to lock more memory than
    /// the limit permitted. The function returns an error.
    ///
    /// [`RLIMIT_MEMLOCK`]: nix::sys::resource::Resource::RLIMIT_MEMLOCK
    pub fn register(&self) -> io::Result<()> {
        CONTEXT.with(|x| {
            x.handle()
                .as_ref()
                .expect("Not in a runtime context")
                .register_buffers(Rc::clone(&self.inner) as _)
        })
    }

    /// Unregisters this buffer heap.
    ///
    /// This method must be called in the context of a `tokio-uring` runtime,
    /// where the buffers should have been previously registered.
    ///
    /// This operation invalidates any `FixedBuf` handles checked out from
    /// this registry instance. Continued use of such handles in I/O
    /// operations may result in an error.
    ///
    /// # Errors
    ///
    /// If another collection of buffers is currently registered in the context
    /// of the `tokio-uring` runtime this call is made in, the function returns
    /// an error. Calling `unregister` when no `FixedBufAllocator` is currently
    /// registered on this runtime also returns an error.
    pub fn unregister(&self) -> io::Result<()> {
        CONTEXT.with(|x| {
            x.handle()
                .as_ref()
                .expect("Not in a runtime context")
                .unregister_buffers(Rc::clone(&self.inner) as _)
        })
    }

    /// Returns a buffer of requested capacity from this allocator
    /// that is not currently owned by any other [`FixedBuf`] handle.
    /// If no such free buffer is available, returns `None`.
    ///
    /// The buffer is released to be available again once the
    /// returned `FixedBuf` handle has been dropped. An I/O operation
    /// using the buffer takes ownership of it and returns it once completed,
    /// preventing shared use of the buffer while the operation is in flight.
    ///
    /// An application should not rely on any particular order
    /// in which available buffers are retrieved.
    pub fn allocate_fixed(&self, cap: usize) -> Option<FixedBuf> {
        let mut inner = self.inner.borrow_mut();
        inner.allocate_fixed(cap).map(|buf| {
            let allocator = Rc::clone(&self.inner);
            // Safety: the validity of buffer data is ensured by
            // Inner::allocate_fixed
            unsafe { FixedBuf::new(allocator, buf) }
        })
    }
}

struct Inner {
    // Pointer to the start of the heap.
    heap: NonNull<u8>,
    // Layout of the heap
    layout: Layout,
    // Buddy allocator for the heap
    allocator: BuddyAlloc,
    // iovec mapped to the heap for registration
    // for now we are limited to just a single iovec
    // if it crosses the limit of io_uring that is a registration error
    iovec: Vec<iovec>,
}

impl Inner {
    fn with_heap(heap: NonNull<u8>, layout: Layout) -> Self {
        let size = layout.size();

        let iovec = vec![iovec {
            iov_base: heap.as_ptr() as _,
            iov_len: layout.size(),
        }];

        Inner {
            heap,
            layout,
            allocator: unsafe {
                BuddyAlloc::new(BuddyAllocParam::new(
                    heap.as_ptr() as _,
                    size,
                    layout.align(),
                ))
            },
            iovec,
        }
    }

    // If there is available space in the heap, return an AllocatedBuf of the requested size,
    // otherwise return None
    fn allocate_fixed(&mut self, cap: usize) -> Option<AllocatedBuf> {
        NonNull::new(self.allocator.malloc(cap)).map(|buf| {
            let iovec = iovec {
                iov_base: buf.as_ptr() as _,
                iov_len: cap,
            };
            let init_len = cap;
            let index = 0; // just one possible registration currently
            AllocatedBuf {
                iovec,
                init_len,
                index,
            }
        })
    }
}

impl FixedBuffers for Inner {
    fn iovecs(&self) -> &[iovec] {
        self.iovec.as_slice()
    }
}

impl AllocatableBuffers for Inner {
    unsafe fn free(&mut self, buf: AllocatedBuf) {
        self.allocator.free(buf.iovec.iov_base as _)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.heap.as_ptr(), self.layout);
        }
    }
}
