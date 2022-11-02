//! Buffers pre-registered with the kernel.
//!
//! This module provides facilities for registering in-memory buffers with
//! the `tokio-uring` runtime. Operations like [`File::read_fixed_at`][rfa] and
//! [`File::write_fixed_at`][wfa] make use of buffers pre-mapped by
//! the kernel to reduce per-I/O overhead.
//! The [`BufRegistry::register`] method is used to register a collection of
//! buffers with the kernel; it must be called before any of the [`FixedBuf`]
//! handles to the collection's buffers can be used with I/O operations.
//!
//! [rfa]: crate::fs::File::read_fixed_at
//! [wfa]: crate::fs::File::write_fixed_at

mod buffers;
pub(crate) use self::buffers::FixedBuffers;

mod handle;
pub use handle::FixedBuf;

mod registry;
pub use registry::FixedBufRegistry;
