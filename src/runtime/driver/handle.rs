use io_uring::squeue;
use std::cell::RefCell;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::{Rc, Weak};
use std::task::{Context, Poll};

use crate::buf::fixed::FixedBuffers;
use crate::runtime::driver::op::{Completable, Lifecycle, MultiCQEFuture, Op, Updateable};
use crate::runtime::driver::Driver;

#[derive(Clone)]
pub(crate) struct Handle {
    inner: Rc<RefCell<Driver>>,
}

#[derive(Clone)]
pub(crate) struct WeakHandle {
    inner: Weak<RefCell<Driver>>,
}

impl Handle {
    pub(crate) fn new(b: &crate::Builder) -> io::Result<Self> {
        Ok(Self {
            inner: Rc::new(RefCell::new(Driver::new(b)?)),
        })
    }

    pub(crate) fn tick(&self) {
        self.inner.borrow_mut().tick()
    }

    pub(crate) fn flush(&self) -> io::Result<usize> {
        self.inner.borrow_mut().uring.submit()
    }

    pub(crate) fn register_buffers(
        &self,
        buffers: Rc<RefCell<dyn FixedBuffers>>,
    ) -> io::Result<()> {
        let mut driver = self.inner.borrow_mut();

        driver
            .uring
            .submitter()
            .register_buffers(buffers.borrow().iovecs())?;

        driver.fixed_buffers = Some(buffers);
        Ok(())
    }

    pub(crate) fn unregister_buffers(
        &self,
        buffers: Rc<RefCell<dyn FixedBuffers>>,
    ) -> io::Result<()> {
        let mut driver = self.inner.borrow_mut();

        if let Some(currently_registered) = &driver.fixed_buffers {
            if Rc::ptr_eq(&buffers, currently_registered) {
                driver.uring.submitter().unregister_buffers()?;
                driver.fixed_buffers = None;
                return Ok(());
            }
        }
        Err(io::Error::new(
            io::ErrorKind::Other,
            "fixed buffers are not currently registered",
        ))
    }

    /// Submit an operation to uring.
    ///
    /// `state` is stored during the operation tracking any state submitted to
    /// the kernel.
    pub(crate) fn submit_op<T, S, F>(&self, mut data: T, f: F) -> io::Result<Op<T, S>>
    where
        T: Completable,
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        let mut driver = self.inner.borrow_mut();
        let index = driver.ops.insert();

        // Configure the SQE
        let sqe = f(&mut data).user_data(index as _);

        // Create the operation
        let op = Op::new(self.into(), data, index);

        // Push the new operation
        while unsafe { driver.uring.submission().push(&sqe).is_err() } {
            // If the submission queue is full, flush it to the kernel
            driver.submit()?;
        }

        Ok(op)
    }

    pub(crate) fn poll_op<T>(&self, op: &mut Op<T>, cx: &mut Context<'_>) -> Poll<T::Output>
    where
        T: Unpin + 'static + Completable,
    {
        use std::mem;

        let mut driver = self.inner.borrow_mut();

        let (lifecycle, _) = driver
            .ops
            .get_mut(op.index)
            .expect("invalid internal state");

        match mem::replace(lifecycle, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) if !waker.will_wake(cx.waker()) => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) => {
                *lifecycle = Lifecycle::Waiting(waker);
                Poll::Pending
            }
            Lifecycle::Ignored(..) => unreachable!(),
            Lifecycle::Completed(cqe) => {
                driver.ops.remove(op.index);
                op.index = usize::MAX;
                Poll::Ready(op.data.take().unwrap().complete(cqe))
            }
            Lifecycle::CompletionList(..) => {
                unreachable!("No `more` flag set for SingleCQE")
            }
        }
    }

    pub(crate) fn poll_multishot_op<T>(
        &self,
        op: &mut Op<T, MultiCQEFuture>,
        cx: &mut Context<'_>,
    ) -> Poll<T::Output>
    where
        T: Unpin + 'static + Completable + Updateable,
    {
        use std::mem;

        let mut driver = self.inner.borrow_mut();

        let (lifecycle, completions) = driver
            .ops
            .get_mut(op.index)
            .expect("invalid internal state");

        match mem::replace(lifecycle, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) if !waker.will_wake(cx.waker()) => {
                *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) => {
                *lifecycle = Lifecycle::Waiting(waker);
                Poll::Pending
            }
            Lifecycle::Ignored(..) => unreachable!(),
            Lifecycle::Completed(cqe) => {
                // This is possible. We may have previously polled a CompletionList,
                // and the final CQE registered as Completed
                driver.ops.remove(op.index);
                op.index = usize::MAX;
                Poll::Ready(op.data.take().unwrap().complete(cqe))
            }
            Lifecycle::CompletionList(indices) => {
                let mut data = op.data.take().unwrap();
                let mut status = Poll::Pending;
                // Consume the CqeResult list, calling update on the Op on all Cqe's flagged `more`
                // If the final Cqe is present, clean up and return Poll::Ready
                for cqe in indices.into_list(completions) {
                    if io_uring::cqueue::more(cqe.flags) {
                        data.update(cqe);
                    } else {
                        status = Poll::Ready(cqe);
                        break;
                    }
                }
                match status {
                    Poll::Pending => {
                        // We need more CQE's. Restore the op state
                        let _ = op.data.insert(data);
                        *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                        Poll::Pending
                    }
                    Poll::Ready(cqe) => {
                        driver.ops.remove(op.index);
                        op.index = usize::MAX;
                        Poll::Ready(data.complete(cqe))
                    }
                }
            }
        }
    }

    pub(crate) fn remove_op<T, CqeType>(&self, op: &mut Op<T, CqeType>) {
        use std::mem;

        let mut driver = self.inner.borrow_mut();

        // Get the Op Lifecycle state from the driver
        let (lifecycle, completions) = match driver.ops.get_mut(op.index) {
            Some(val) => val,
            None => {
                // Op dropped after the driver
                return;
            }
        };

        match mem::replace(lifecycle, Lifecycle::Submitted) {
            Lifecycle::Submitted | Lifecycle::Waiting(_) => {
                *lifecycle = Lifecycle::Ignored(Box::new(op.data.take()));
            }
            Lifecycle::Completed(..) => {
                driver.ops.remove(op.index);
            }
            Lifecycle::CompletionList(indices) => {
                // Deallocate list entries, recording if more CQE's are expected
                let more = {
                    let mut list = indices.into_list(completions);
                    io_uring::cqueue::more(list.peek_end().unwrap().flags)
                    // Dropping list deallocates the list entries
                };
                if more {
                    // If more are expected, we have to keep the op around
                    *lifecycle = Lifecycle::Ignored(Box::new(op.data.take()));
                } else {
                    driver.ops.remove(op.index);
                }
            }
            Lifecycle::Ignored(..) => unreachable!(),
        }
    }
}

impl WeakHandle {
    pub(crate) fn upgrade(&self) -> Option<Handle> {
        Some(Handle {
            inner: self.inner.upgrade()?,
        })
    }
}

impl AsRawFd for Handle {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.borrow().uring.as_raw_fd()
    }
}

impl From<Driver> for Handle {
    fn from(driver: Driver) -> Self {
        Self {
            inner: Rc::new(RefCell::new(driver)),
        }
    }
}

impl From<&Handle> for WeakHandle {
    fn from(handle: &Handle) -> Self {
        Self {
            inner: Rc::downgrade(&handle.inner),
        }
    }
}
