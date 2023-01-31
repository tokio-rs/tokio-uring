//! Buffers pre-registered with the kernel.
//!
//! This module provides facilities for registering in-memory buffers with
//! the `tokio-uring` runtime. Operations like [`File::read_fixed_at`][rfa] and
//! [`File::write_fixed_at`][wfa] make use of buffers pre-mapped by
//! the kernel to reduce per-I/O overhead.
//!
//! Three kinds of buffer collections are provided: [`FixedBufRegistry`],
//! [`FixedBufPool`] and [`FixedBufAllocator`], realizing two different patterns of buffer management.
//!
//! The `register` method on either of these types is used to register a
//! collection of buffers with the kernel. It must be called before any of
//! the [`FixedBuf`] handles to the collection's buffers can be used with
//! I/O operations.
//!
//! [rfa]: crate::fs::File::read_fixed_at
//! [wfa]: crate::fs::File::write_fixed_at

mod allocator;
pub use allocator::FixedBufAllocator;

mod handle;
pub use handle::FixedBuf;

mod buffers;
pub(crate) use buffers::FixedBuffers;

mod pool;
pub use pool::FixedBufPool;

mod registry;
pub use registry::FixedBufRegistry;
