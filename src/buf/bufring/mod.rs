//! A buf_ring pool of buffers registered with the kernel.
//!
//! This module provides the [`BufRing`] and [`Builder`] to allow
//! using the `buf_ring` feature of the kernel's `io_uring` device.
//!
//! The [`BufRing`] is this library's only implementation of the device's more general `Provided
//! Buffers` feature where some device operations can work with buffers that had been provided to
//! the device at an earlier point, rather than as part of the operation itself.
//!
//! Operations like [`crate::net::TcpStream::recv_provbuf`] make use of the `buf_ring`. This
//! operation does not take a buffer as input, but does return a buffer when successful. Once the
//! buffer is dropped, it is returned to the `buf_ring`.

pub(crate) mod ring;

pub use ring::BufRing;
pub use ring::Builder;
