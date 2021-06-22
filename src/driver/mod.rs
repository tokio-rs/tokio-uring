// mod accept;

mod close;
pub(crate) use close::Close;

mod op;
pub(crate) use op::Op;

mod open;

mod pool;
pub(crate) use pool::Pool;

mod read;

mod shared_fd;
pub(crate) use shared_fd::SharedFd;

/*
mod stream;
pub(crate) use stream::Stream;
*/

mod util;

mod write;

use io_uring::IoUring;
use scoped_tls::scoped_thread_local;
use slab::Slab;
use std::cell::RefCell;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;

pub(crate) struct Driver {
    inner: Handle,
}

type Handle = Rc<RefCell<Inner>>;

struct Inner {
    /// In-flight operations
    ops: Slab<op::Lifecycle>,

    pool: Pool,

    /// IoUring bindings
    uring: IoUring,
}

scoped_thread_local!(static CURRENT: Rc<RefCell<Inner>>);

impl Driver {
    pub(crate) fn new() -> io::Result<Driver> {
        let mut uring = IoUring::new(256)?;
        let pool = Pool::new(256, 4096 * 2);

        pool.provide_buffers(&mut uring)?;

        let inner = Rc::new(RefCell::new(Inner {
            ops: Slab::with_capacity(64),
            pool,
            uring,
        }));

        Ok(Driver { inner })
    }

    // Enter the driver context. This enables using uring types.
    pub(crate) fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        CURRENT.set(&self.inner, || f())
    }

    pub(crate) fn tick(&self) {
        let mut inner = self.inner.borrow_mut();
        let inner = &mut *inner;

        // TODO: Error?
        // inner.uring.submit_and_wait(1).unwrap();

        let mut cq = inner.uring.completion();
        cq.sync();

        for cqe in cq {
            if cqe.user_data() == u64::MAX {
                // Result of the cancellation action. There isn't anything we
                // need to do here. We must wait for the CQE for the operation
                // that was canceled.
                continue;
            }

            let index = cqe.user_data() as _;

            if inner.ops[index].complete(cqe) {
                inner.ops.remove(index);
            }
        }
    }
}

impl AsRawFd for Driver {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.borrow().uring.as_raw_fd()
    }
}
