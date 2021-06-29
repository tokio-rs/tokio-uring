use crate::driver;

use io_uring::{cqueue, squeue};
use std::cell::RefCell;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// In-flight operation
pub(crate) struct Op<T: 'static> {
    // Driver running the operation
    pub(super) driver: Rc<RefCell<driver::Inner>>,

    // Operation index in the slab
    pub(super) index: usize,

    // Per-operation data
    data: Option<T>,
}

/// Operation completion. Returns stored state with the result of the operation.
#[derive(Debug)]
pub(crate) struct Completion<T> {
    pub(crate) data: T,
    pub(crate) result: io::Result<u32>,
    pub(crate) flags: u32,
}

pub(crate) enum Lifecycle {
    /// The operation has been submitted to uring and is currently in-flight
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result. The state
    /// must be passed to the driver and held until the operation completes.
    Ignored(Box<dyn std::any::Any>),

    /// The operation has completed.
    Completed(cqueue::Entry),
}

impl<T> Op<T> {
    /// Submit an operation to uring.
    ///
    /// `state` is stored during the operation tracking any state submitted to
    /// the kernel.
    pub(super) fn submit_with<F>(data: T, f: F) -> io::Result<Op<T>>
    where
        F: FnOnce() -> squeue::Entry,
    {
        driver::CURRENT.with(|inner_rc| {
            let mut inner = inner_rc.borrow_mut();
            let inner = &mut *inner;

            // Store the operation state
            let index = inner.ops.insert();

            // Configure the SQE
            let sqe = f().user_data(index as _);

            let (submitter, mut sq, _) = inner.uring.split();

            if sq.is_full() {
                // TODO: we probably want to do a busy loop here as well.
                submitter.submit()?;
                sq.sync();
            }

            if unsafe { sq.push(&sqe).is_err() } {
                unimplemented!("when is this hit?");
            }

            drop(sq);
            loop {
                match inner.uring.submit() {
                    Ok(_) => break,
                    // TODO: currently, the BUSY error is represented as
                    // `Other`. This should be broken out and disambiguated.
                    Err(ref e) if e.kind() == io::ErrorKind::Other => {
                        inner.tick();
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }

            Ok(Op {
                driver: inner_rc.clone(),
                index,
                data: Some(data),
            })
        })
    }

    /// Try submitting an operation to uring
    pub(super) fn try_submit_with<F>(data: T, f: F) -> io::Result<Op<T>>
    where
        F: FnOnce() -> squeue::Entry,
    {
        if driver::CURRENT.is_set() {
            Op::submit_with(data, f)
        } else {
            Err(io::ErrorKind::Other.into())
        }
    }
}

impl<T> Future for Op<T>
where
    T: Unpin + 'static,
{
    type Output = Completion<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::mem;

        let me = &mut *self;
        let mut inner = me.driver.borrow_mut();
        let lifecycle = inner.ops.get_mut(me.index).expect("invalid internal state");

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
                inner.ops.remove(me.index);
                me.index = usize::MAX;

                Poll::Ready(Completion {
                    data: me.data.take().expect("unexpected operation state"),
                    result: if cqe.result() >= 0 {
                        Ok(cqe.result() as u32)
                    } else {
                        Err(io::Error::from_raw_os_error(-cqe.result()))
                    },
                    flags: cqe.flags(),
                })
            }
        }
    }
}

impl<T> Drop for Op<T> {
    fn drop(&mut self) {
        let mut inner = self.driver.borrow_mut();
        let lifecycle = match inner.ops.get_mut(self.index) {
            Some(lifecycle) => lifecycle,
            None => return,
        };

        match lifecycle {
            Lifecycle::Submitted | Lifecycle::Waiting(_) => {
                *lifecycle = Lifecycle::Ignored(Box::new(self.data.take()));
            }
            Lifecycle::Completed(_) => {
                inner.ops.remove(self.index);
            }
            Lifecycle::Ignored(..) => unreachable!(),
        }
    }
}

impl Lifecycle {
    pub(super) fn complete(&mut self, cqe: cqueue::Entry) -> bool {
        use std::mem;

        match mem::replace(self, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                *self = Lifecycle::Completed(cqe);
                false
            }
            Lifecycle::Waiting(waker) => {
                *self = Lifecycle::Completed(cqe);
                waker.wake();
                false
            }
            Lifecycle::Ignored(..) => true,
            Lifecycle::Completed(_) => unreachable!("invalid operation state"),
        }
    }
}
