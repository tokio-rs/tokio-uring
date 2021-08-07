//! Traits, helpers, and type definitions for asynchronous I/O functionality.
//!
//! This module is the asynchronous version of `std::io`. Primarily, it
//! defines two traits, [`AsyncRead`] and [`AsyncWrite`], which are asynchronous
//! versions of the [`Read`] and [`Write`] traits in the standard library.
//! In addition, there is also the AsyncReadAt and AsyncWriteAt, which a
//! position can be given.
//!
//! [`AsyncRead`]: trait@AsyncRead
//! [`AsyncRead`]: trait@AsyncReadAt
//! [`AsyncWrite`]: trait@AsyncWrite
//! [`AsyncWrite`]: trait@AsyncWriteAt
//! [`Read`]: std::io::Read
//! [`Write`]: std::io::Write

mod read;
pub use read::{AsyncRead, AsyncReadAt};

mod write;
pub use write::{AsyncWrite, AsyncWriteAt};
