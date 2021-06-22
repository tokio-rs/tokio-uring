mod driver;

pub mod buf;
pub mod fs;
pub mod io;
// pub mod net;
pub mod runtime;

use std::future::Future;

/// Start an `io_uring` enabled Tokio runtime
pub fn start<F: Future>(future: F) -> F::Output {
    let mut rt = runtime::Runtime::new().unwrap();
    rt.block_on(future)
}

pub type BufResult<T, B> = (std::io::Result<T>, B);

pub type BufMutResult<T, B> = (std::io::Result<T>, B);
