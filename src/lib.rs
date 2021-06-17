mod driver;

pub mod buf;
pub mod io;
pub mod fs;
// pub mod net;
pub mod runtime;

use std::future::Future;

/// Start an `io_uring` enabled Tokio runtime
pub fn start<F: Future>(future: F) -> F::Output {
    let mut rt = runtime::Runtime::new().unwrap();
    rt.block_on(future)
}

pub type BufResult<T> = (std::io::Result<T>, buf::Slice);

pub type BufMutResult<T> = (std::io::Result<T>, buf::SliceMut);
