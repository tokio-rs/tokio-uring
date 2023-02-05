//! Tokio-uring provides a safe [io-uring] interface for the Tokio runtime. The
//! library requires Linux kernel 5.10 or later.
//!
//! [io-uring]: https://kernel.dk/io_uring.pdf
//!
//! # Getting started
//!
//! Using `tokio-uring` requires starting a [`tokio-uring`] runtime. This
//! runtime internally manages the main Tokio runtime and a `io-uring` driver.
//!
//! ```no_run
//! use tokio_uring::fs::File;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     tokio_uring::start(async {
//!         // Open a file
//!         let file = File::open("hello.txt").await?;
//!
//!         let buf = vec![0; 4096];
//!         // Read some data, the buffer is passed by ownership and
//!         // submitted to the kernel. When the operation completes,
//!         // we get the buffer back.
//!         let (res, buf) = file.read_at(buf, 0).await;
//!         let n = res?;
//!
//!         // Display the contents
//!         println!("{:?}", &buf[..n]);
//!
//!         Ok(())
//!     })
//! }
//! ```
//!
//! Under the hood, `tokio_uring::start` starts a [`current-thread`] Runtime.
//! For concurrency, spawn multiple threads, each with a `tokio-uring` runtime.
//! The `tokio-uring` resource types are optimized for single-threaded usage and
//! most are `!Sync`.
//!
//! # Submit-based operations
//!
//! Unlike Tokio proper, `io-uring` is based on submission based operations.
//! Ownership of resources are passed to the kernel, which then performs the
//! operation. When the operation completes, ownership is passed back to the
//! caller. Because of this difference, the `tokio-uring` APIs diverge.
//!
//! For example, in the above example, reading from a `File` requires passing
//! ownership of the buffer.
//!
//! # Closing resources
//!
//! With `io-uring`, closing a resource (e.g. a file) is an asynchronous
//! operation. Because Rust does not support asynchronous drop yet, resource
//! types provide an explicit `close()` function. If the `close()` function is
//! not called, the resource will still be closed on drop, but the operation
//! will happen in the background. There is no guarantee as to **when** the
//! implicit close-on-drop operation happens, so it is recommended to explicitly
//! call `close()`.

#![warn(missing_docs)]

macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

#[macro_use]
mod future;
mod io;
mod runtime;

pub mod buf;
pub mod fs;
pub mod net;

pub use runtime::spawn;
pub use runtime::Runtime;

use crate::runtime::driver::op::Op;
use crate::runtime::driver::{CEntry, SEntry};
use std::future::Future;

/// Starts an `io_uring` enabled Tokio runtime.
///
/// All `tokio-uring` resource types must be used from within the context of a
/// runtime. The `start` method initializes the runtime and runs it for the
/// duration of `future`.
///
/// The `tokio-uring` runtime is compatible with all Tokio, so it is possible to
/// run Tokio based libraries (e.g. hyper) from within the tokio-uring runtime.
/// A `tokio-uring` runtime consists of a Tokio `current_thread` runtime and an
/// `io-uring` driver. All tasks spawned on the `tokio-uring` runtime are
/// executed on the current thread. To add concurrency, spawn multiple threads,
/// each with a `tokio-uring` runtime.
///
/// # Examples
///
/// Basic usage
///
/// ```no_run
/// use tokio_uring::fs::File;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     tokio_uring::start(async {
///         // Open a file
///         let file = File::open("hello.txt").await?;
///
///         let buf = vec![0; 4096];
///         // Read some data, the buffer is passed by ownership and
///         // submitted to the kernel. When the operation completes,
///         // we get the buffer back.
///         let (res, buf) = file.read_at(buf, 0).await;
///         let n = res?;
///
///         // Display the contents
///         println!("{:?}", &buf[..n]);
///
///         Ok(())
///     })
/// }
/// ```
///
/// Using Tokio types from the `tokio-uring` runtime
///
///
/// ```no_run
/// use tokio::net::TcpListener;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     tokio_uring::start(async {
///         let listener = TcpListener::bind("127.0.0.1:8080").await?;
///
///         loop {
///             let (socket, _) = listener.accept().await?;
///             // process socket
///         }
///     })
/// }
/// ```
pub fn start<F: Future>(future: F) -> F::Output {
    let rt = runtime::Runtime::new(&builder()).unwrap();
    rt.block_on(future)
}

/// Creates and returns an io_uring::Builder that can then be modified
/// through its implementation methods.
///
/// This function is provided to avoid requiring the user of this crate from
/// having to use the io_uring crate as well. Refer to Builder::start example
/// for its intended usage.
pub fn uring_builder() -> io_uring::Builder<SEntry, CEntry> {
    io_uring::IoUring::generic_builder()
}

/// Builder API that can create and start the `io_uring` runtime with non-default parameters,
/// while abstracting away the underlying io_uring crate.
// #[derive(Clone, Default)]
pub struct Builder {
    entries: u32,
    urb: io_uring::Builder<SEntry, CEntry>,
}

/// Constructs a [`Builder`] with default settings.
///
/// Use this to alter submission and completion queue parameters, and to create the io_uring
/// Runtime.
///
/// Refer to [`Builder::start`] for an example.
pub fn builder() -> Builder {
    Builder {
        entries: 256,
        urb: io_uring::IoUring::generic_builder(),
    }
}

impl Builder {
    /// Sets the number of Submission Queue entries in uring.
    ///
    /// The default value is 256.
    /// The kernel requires the number of submission queue entries to be a power of two,
    /// and that it be less than the number of completion queue entries.
    /// This function will adjust the `cq_entries` value to be at least 2 times `sq_entries`
    pub fn entries(&mut self, sq_entries: u32) -> &mut Self {
        self.entries = sq_entries;
        self
    }

    /// Replaces the default [`io_uring::Builder`], which controls the settings for the
    /// inner `io_uring` API.
    ///
    /// Refer to the Builder start method for an example.
    /// Refer to the io_uring::builder documentation for all the supported methods.
    pub fn uring_builder(&mut self, b: &io_uring::Builder<SEntry, CEntry>) -> &mut Self {
        self.urb = b.clone();
        self
    }

    /// Starts an `io_uring` enabled Tokio runtime.
    ///
    /// # Examples
    ///
    /// Creating a uring driver with only 64 submission queue entries but
    /// many more completion queue entries.
    ///
    /// ```no_run
    /// use tokio::net::TcpListener;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     tokio_uring::builder()
    ///         .entries(64)
    ///         .uring_builder(tokio_uring::uring_builder()
    ///             .setup_cqsize(1024)
    ///             )
    ///         .start(async {
    ///             let listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    ///             loop {
    ///                 let (socket, _) = listener.accept().await?;
    ///                 // process socket
    ///             }
    ///         }
    ///     )
    /// }
    /// ```
    pub fn start<F: Future>(&self, future: F) -> F::Output {
        let rt = runtime::Runtime::new(self).unwrap();
        rt.block_on(future)
    }
}

/// A specialized `Result` type for `io-uring` operations with buffers.
///
/// This type is used as a return value for asynchronous `io-uring` methods that
/// require passing ownership of a buffer to the runtime. When the operation
/// completes, the buffer is returned whether or not the operation completed
/// successfully.
///
/// # Examples
///
/// ```no_run
/// use tokio_uring::fs::File;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     tokio_uring::start(async {
///         // Open a file
///         let file = File::open("hello.txt").await?;
///
///         let buf = vec![0; 4096];
///         // Read some data, the buffer is passed by ownership and
///         // submitted to the kernel. When the operation completes,
///         // we get the buffer back.
///         let (res, buf) = file.read_at(buf, 0).await;
///         let n = res?;
///
///         // Display the contents
///         println!("{:?}", &buf[..n]);
///
///         Ok(())
///     })
/// }
/// ```
pub type BufResult<T, B> = (std::io::Result<T>, B);

/// The simplest possible operation. Just posts a completion event, nothing else.
///
/// This has a place in benchmarking and sanity checking uring.
///
/// # Examples
///
/// ```no_run
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     tokio_uring::start(async {
///         // Place a NoOp on the ring, and await completion event
///         tokio_uring::no_op().await?;
///         Ok(())
///     })
/// }
/// ```
pub async fn no_op() -> std::io::Result<()> {
    let op = Op::<io::NoOp>::no_op().unwrap();
    op.await
}
