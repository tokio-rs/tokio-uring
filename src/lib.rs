mod driver;

pub mod buf;
pub mod io;
pub mod fs;
pub mod net;
pub mod runtime;

use std::future::Future;

/// Start an `io_uring` enabled Tokio runtime
pub fn start<F: Future>(future: F) -> F::Output {
    let mut rt = runtime::Runtime::new().unwrap();
    rt.block_on(future)
}

type BufResult<T> = (std::io::Result<T>, buf::Slice);

/*
pub fn run() {
    use mio::*;
    use mio::unix::*;
    use std::fs::*;
    use std::os::unix::io::*;
    use std::path::PathBuf;

    let mut io_uring = iou::IoUring::new(32).unwrap();

    // Register the uring w/ epoll
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(256);
    poll.registry().register(&mut SourceFd(&io_uring.raw_fd()), Token(0), Interest::READABLE | Interest::WRITABLE).unwrap();

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("Cargo.toml");

    let file = File::open(&path).unwrap();
    let mut buf = [0; 4096];

    let mut sq = io_uring.sq();
    let mut sqe = sq.prepare_sqe().unwrap();

    unsafe {
        sqe.prep_read(file.as_raw_fd(), &mut buf[..], 0);
    }

    let res = sq.submit().unwrap();
    println!("res = {:?}; peek = {:?}", res, io_uring.cq().peek_for_cqe().is_some());

    poll.poll(&mut events, None).unwrap();
    assert!(!events.is_empty());

    for event in &events {
        println!("EVENT = {:?}", event);
    }

    let n = {
        let mut cq = io_uring.cq();
        let cqe = cq.peek_for_cqe().unwrap();
        cqe.result().unwrap() as usize
    };

    println!("{}", std::str::from_utf8(&buf[..n]).unwrap());
}
*/
