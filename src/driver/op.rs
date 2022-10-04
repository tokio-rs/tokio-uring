use std::cell::RefCell;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use io_uring::squeue;
use smallvec::{smallvec, SmallVec};

use crate::driver;

/// In-flight operation
pub(crate) struct Op<T: Completable + 'static> {
    // Driver running the operation
    pub(super) driver: Rc<RefCell<driver::Inner>>,

    // Operation index in the slab
    pub(super) index: usize,

    // Per-operation data
    data: Option<T>,
}

pub(crate) trait Completable {
    type Output;
    fn update(self, result: io::Result<u32>, flags: u32) -> Update<Self::Output, Self> where Self: Sized {
        Update::Finished(self.complete(result, flags))
    }
    fn complete(self, result: io::Result<u32>, flags: u32) -> Self::Output;
}

pub(crate) enum Update<F,M>{
    /// The operation has finished
    Finished(F),

    /// The operation expects more completion events
    More(M),
}

pub(crate) enum Lifecycle {
    /// The operation has been submitted to uring and is currently in-flight
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result. The state
    /// must be passed to the driver and held until the operation completes.
    Ignored(Box<dyn std::any::Any>),

    /// The operation has 1 or more unserviced completion events, and poll has yet to be called
    ///
    /// This allocates if more than the default number of completions (currently 4)
    /// Are received before poll is invoked
    Completed( SmallVec<[(io::Result<u32>, u32); 4]>),
}

impl<T> Op<T>
where
    T: Completable,
{
    /// Create a new operation
    fn new(data: T, inner: &mut driver::Inner, inner_rc: &Rc<RefCell<driver::Inner>>) -> Op<T> {
        Op {
            driver: inner_rc.clone(),
            index: inner.ops.insert(),
            data: Some(data),
        }
    }

    /// Submit an operation to uring.
    ///
    /// `state` is stored during the operation tracking any state submitted to
    /// the kernel.
    pub(super) fn submit_with<F>(data: T, f: F) -> io::Result<Op<T>>
    where
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        driver::CURRENT.with(|inner_rc| {
            let mut inner_ref = inner_rc.borrow_mut();
            let inner = &mut *inner_ref;

            // If the submission queue is full, flush it to the kernel
            if inner.uring.submission().is_full() {
                inner.submit()?;
            }

            // Create the operation
            let mut op = Op::new(data, inner, inner_rc);

            // Configure the SQE
            let sqe = f(op.data.as_mut().unwrap()).user_data(op.index as _);

            {
                let mut sq = inner.uring.submission();

                // Push the new operation
                if unsafe { sq.push(&sqe).is_err() } {
                    unimplemented!("when is this hit?");
                }
            }

            Ok(op)
        })
    }

    /// Try submitting an operation to uring
    pub(super) fn try_submit_with<F>(data: T, f: F) -> io::Result<Op<T>>
    where
        F: FnOnce(&mut T) -> squeue::Entry,
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
    T: Unpin + 'static + Completable,
{
    type Output = T::Output;

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
            Lifecycle::Completed(v) => {
                let data = me.data.take().unwrap();
                let updated = v.into_iter()
                    .fold(Update::More(data), |data, (result, flags)|
                        if let Update::More(data) = data {
                            data.update(result, flags)
                        } else {
                            data
                        }
                    );
                // Check if the operation requires more events to complete
                match updated {
                    Update::Finished(result) => {
                        inner.ops.remove(me.index);
                        me.index = usize::MAX;
                        Poll::Ready(result)
                    },
                    Update::More(op) => {
                        let _ = me.data.insert(op);
                        *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                        Poll::Pending
                    }
                }
            }
        }
    }
}

impl<T: Completable> Drop for Op<T> {
    fn drop(&mut self) {
        use std::mem;

        let mut inner = self.driver.borrow_mut();
        let lifecycle = match inner.ops.get_mut(self.index) {
            Some(lifecycle) => lifecycle,
            None => return,
        };

        match mem::replace(lifecycle, Lifecycle::Submitted) {
            Lifecycle::Submitted | Lifecycle::Waiting(_) => {
                *lifecycle = Lifecycle::Ignored(Box::new(self.data.take()));
            }
            Lifecycle::Completed(v) => {
                let data = self.data.take().unwrap();
                let updated = v.into_iter()
                    .fold(Update::More(data), |data, (result, flags)|
                        if let Update::More(data) = data {
                            data.update(result, flags)
                        } else {
                            data
                        }
                    );
                // Check if the operation requires more events to complete
                match updated {
                    Update::Finished(_) => {
                        inner.ops.remove(self.index);
                    }
                    Update::More(op) => {
                        *lifecycle = Lifecycle::Ignored(Box::new(op));
                    }
                }
            }
            Lifecycle::Ignored(..) => unreachable!(),
        }
    }
}

impl Lifecycle {
    pub(super) fn complete(&mut self, result: io::Result<u32>, flags: u32) -> bool {
        use std::mem;

        match mem::replace(self, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                *self = Lifecycle::Completed(smallvec!((result, flags)));
                false
            }
            Lifecycle::Waiting(waker) => {
                *self = Lifecycle::Completed(smallvec!((result, flags)));
                waker.wake();
                false
            }
            Lifecycle::Ignored(op) => {
                if io_uring::cqueue::more(flags){
                    *self = Lifecycle::Ignored(op);
                    false
                } else {
                    true
                }
            },
            Lifecycle::Completed(mut v) => {
                // We've run a data race between a multi-completion event and
                // the first call to poll().
                v.push((result,flags));
                *self = Lifecycle::Completed(v);
                false
            }

        }
    }
}

#[cfg(test)]
mod test {
    use std::rc::Rc;

    use tokio_test::{assert_pending, assert_ready, task};

    use super::*;

    #[derive(Debug)]
    pub(crate) struct Completion {
        result: io::Result<u32>,
        flags: u32,
        data: Rc<()>,
    }

    impl Completable for Rc<()> {
        type Output = Completion;

        fn complete(self, result: io::Result<u32>, flags: u32) -> Self::Output {
            Completion {
                result,
                flags,
                data: self.clone(),
            }
        }
    }

    #[test]
    fn op_stays_in_slab_on_drop() {
        let (op, driver, data) = init();
        drop(op);

        assert_eq!(2, Rc::strong_count(&data));

        assert_eq!(1, driver.num_operations());
        release(driver);
    }

    #[test]
    fn poll_op_once() {
        let (op, driver, data) = init();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());
        assert_eq!(2, Rc::strong_count(&data));

        complete(&op, Ok(1));
        assert_eq!(1, driver.num_operations());
        assert_eq!(2, Rc::strong_count(&data));

        assert!(op.is_woken());
        let Completion {
            result,
            flags,
            data: d,
        } = assert_ready!(op.poll());
        assert_eq!(2, Rc::strong_count(&data));
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        drop(d);
        assert_eq!(1, Rc::strong_count(&data));

        drop(op);
        assert_eq!(0, driver.num_operations());

        release(driver);
    }

    #[test]
    fn poll_op_twice() {
        let (op, driver, ..) = init();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());
        assert_pending!(op.poll());

        complete(&op, Ok(1));

        assert!(op.is_woken());
        let Completion { result, flags, .. } = assert_ready!(op.poll());
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        release(driver);
    }

    #[test]
    fn poll_change_task() {
        let (op, driver, ..) = init();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());

        let op = op.into_inner();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());

        complete(&op, Ok(1));

        assert!(op.is_woken());
        let Completion { result, flags, .. } = assert_ready!(op.poll());
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        release(driver);
    }

    #[test]
    fn complete_before_poll() {
        let (op, driver, data) = init();
        let mut op = task::spawn(op);
        complete(&op, Ok(1));
        assert_eq!(1, driver.num_operations());
        assert_eq!(2, Rc::strong_count(&data));

        let Completion { result, flags, .. } = assert_ready!(op.poll());
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        drop(op);
        assert_eq!(0, driver.num_operations());

        release(driver);
    }

    #[test]
    fn complete_after_drop() {
        let (op, driver, data) = init();
        let index = op.index;
        drop(op);

        assert_eq!(2, Rc::strong_count(&data));

        assert_eq!(1, driver.num_operations());
        driver.inner.borrow_mut().ops.complete(index, Ok(1), 0);
        assert_eq!(1, Rc::strong_count(&data));
        assert_eq!(0, driver.num_operations());
        release(driver);
    }

    fn init() -> (Op<Rc<()>>, crate::driver::Driver, Rc<()>) {
        use crate::driver::Driver;

        let driver = Driver::new(&crate::builder()).unwrap();
        let handle = driver.inner.clone();
        let data = Rc::new(());

        let op = {
            let mut inner = handle.borrow_mut();
            Op::new(data.clone(), &mut inner, &handle)
        };

        (op, driver, data)
    }

    fn complete(op: &Op<Rc<()>>, result: io::Result<u32>) {
        op.driver.borrow_mut().ops.complete(op.index, result, 0);
    }

    fn release(driver: crate::driver::Driver) {
        // Clear ops, we aren't really doing any I/O
        driver.inner.borrow_mut().ops.0.clear();
    }
}
