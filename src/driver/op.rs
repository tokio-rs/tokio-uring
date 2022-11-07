use std::future::Future;
use std::io;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use io_uring::{
    cqueue,
    squeue::{self, Entry},
};

mod slab_list;

use slab::Slab;
use slab_list::{SlabListEntry, SlabListIndices};

use crate::runtime::CONTEXT;
use crate::util::PhantomUnsendUnsync;

/// A SlabList is used to hold unserved completions.
///
/// This is relevant to multi-completion Operations,
/// which require an unknown number of CQE events to be
/// captured before completion.
pub(crate) type Completion = SlabListEntry<CqeResult>;

/// In-flight operation
pub(crate) struct Op<T: 'static, CqeType = SingleCQE> {
    // Operation index in the slab
    pub(super) index: usize,

    // Per-operation data
    data: Option<T>,

    // CqeType marker
    _cqe_type: PhantomData<CqeType>,

    // Make !Send + !Sync
    _phantom: PhantomUnsendUnsync,
}

/// A Marker for Ops which expect only a single completion event
pub(crate) struct SingleCQE;

pub(crate) trait Completable {
    type Output;
    fn complete(self, cqe: CqeResult) -> Self::Output;
}

pub(crate) enum Lifecycle {
    /// The operation has been created, and will be submitted when polled
    Pending(Entry),

    /// The operation has been manually submitted
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result. The state
    /// must be passed to the driver and held until the operation completes.
    Ignored(Box<dyn std::any::Any>),

    /// The operation has completed with a single cqe result
    Completed(CqeResult),

    /// One or more completion results have been recieved
    /// This holds the indices uniquely identifying the list within the slab
    CompletionList(SlabListIndices),
}

impl std::fmt::Debug for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending(_) => f.debug_tuple("Pending").finish(),
            Self::Submitted => f.debug_tuple("Submitted").finish(),
            Self::Waiting(_) => f.debug_tuple("Waiting").finish(),
            Self::Ignored(_) => f.debug_tuple("Ignored").finish(),
            Self::Completed(_) => f.debug_tuple("Completed").finish(),
            Self::CompletionList(_) => f.debug_tuple("CompletionList").finish(),
        }
    }
}

/// A single CQE entry
pub(crate) struct CqeResult {
    pub(crate) result: io::Result<u32>,
    pub(crate) flags: u32,
}

impl From<cqueue::Entry> for CqeResult {
    fn from(cqe: cqueue::Entry) -> Self {
        let res = cqe.result();
        let flags = cqe.flags();
        let result = if res >= 0 {
            Ok(res as u32)
        } else {
            Err(io::Error::from_raw_os_error(-res))
        };
        CqeResult { result, flags }
    }
}

impl<T, CqeType> Op<T, CqeType> {
    /// Create a new operation
    fn new(data: T, index: usize) -> Self {
        Op {
            index,
            data: Some(data),
            _cqe_type: PhantomData,
            _phantom: PhantomData,
        }
    }

    /// Submit an operation to the driver.
    ///
    /// `data` is stored during the operation tracking any state for submission to
    /// the kernel.
    pub(super) fn submit_with<F>(mut data: T, f: F) -> Self
    where
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        CONTEXT.with(|cx| {
            cx.with_driver_mut(|driver| {
                // Get an vacent entry in the Lifecycle slab
                let entry = driver.ops.insert();

                // Configure the Sqe for submission
                let sqe = f(&mut data).user_data(entry.key() as _);

                // Create a pending Lifecycle entry for the Op
                let op = Op::new(data, entry.key());
                entry.insert(Lifecycle::Pending(sqe));

                op
            })
        })
    }

    /// Enqueue an operation on the submission ring, without `await`ing the future
    ///
    /// This is useful when any failure in io_uring submission needs
    /// to be handled at the point of submission.
    pub(super) fn enqueue(self) -> io::Result<Self> {
        CONTEXT
            .with(|runtime_context| {
                runtime_context.with_driver_mut(|driver| {
                    let (lifecycle, _) = driver
                        .ops
                        .get_mut(self.index)
                        .expect("invalid internal state");

                    match std::mem::replace(lifecycle, Lifecycle::Submitted) {
                        Lifecycle::Pending(sqe) => {
                            // Try to push the new operation
                            while unsafe { driver.uring.submission().push(&sqe).is_err() } {
                                // Fail with an IoError if encountered
                                driver.submit()?;
                            }
                        }
                        lc => {
                            let lifecycle = driver.ops.get_mut(self.index).unwrap().0;
                            *lifecycle = lc;
                        }
                    }
                    Ok(())
                })
            })
            .map(|_| self)
    }
}

impl<T> Future for Op<T, SingleCQE>
where
    T: Unpin + 'static + Completable,
{
    type Output = T::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::mem;

        let me = &mut *self;

        CONTEXT.with(|runtime_context| {
            runtime_context.with_driver_mut(|driver| {
                let (lifecycle, _) = driver
                    .ops
                    .get_mut(me.index)
                    .expect("invalid internal state");

                match mem::replace(lifecycle, Lifecycle::Ignored(Box::new(()))) {
                    Lifecycle::Pending(sqe) => {
                        // Try to push the new operation
                        while unsafe { driver.uring.submission().push(&sqe).is_err() } {
                            // If the sqe is full, flush to kernel
                            if let Err(e) = driver.submit() {
                                // Fail if an IoError in encountered
                                let cqe = CqeResult {
                                    result: Err(e),
                                    flags: 0,
                                };
                                driver.ops.remove(me.index);
                                me.index = usize::MAX;
                                return Poll::Ready(me.data.take().unwrap().complete(cqe));
                            }
                        }
                        let lifecycle = driver.ops.get_mut(me.index).unwrap().0;
                        *lifecycle = Lifecycle::Waiting(cx.waker().clone());
                        Poll::Pending
                    }
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
                        driver.ops.remove(me.index);
                        me.index = usize::MAX;
                        Poll::Ready(me.data.take().unwrap().complete(cqe))
                    }
                    Lifecycle::CompletionList(..) => {
                        unreachable!("No `more` flag set for SingleCQE")
                    }
                }
            })
        })
    }
}

/// The operation may have pending cqe's not yet processed.
/// To manage this, the lifecycle associated with the Op may if required
/// be placed in LifeCycle::Ignored state to handle cqe's which arrive after
/// the Op has been dropped.
impl<T, CqeType> Drop for Op<T, CqeType> {
    fn drop(&mut self) {
        use std::mem;

        CONTEXT.with(|runtime_context| {
            runtime_context.with_driver_mut(|driver| {
                // Get the Op Lifecycle state from the driver
                let (lifecycle, completions) = match driver.ops.get_mut(self.index) {
                    Some(val) => val,
                    None => {
                        // Op dropped after the driver
                        return;
                    }
                };

                match mem::replace(lifecycle, Lifecycle::Ignored(Box::new(()))) {
                    Lifecycle::Submitted | Lifecycle::Waiting(_) => {
                        *lifecycle = Lifecycle::Ignored(Box::new(self.data.take()));
                    }
                    Lifecycle::Completed(..) | Lifecycle::Pending(_) => {
                        driver.ops.remove(self.index);
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
                            *lifecycle = Lifecycle::Ignored(Box::new(self.data.take()));
                        } else {
                            driver.ops.remove(self.index);
                        }
                    }
                    Lifecycle::Ignored(..) => unreachable!(),
                }
            })
        })
    }
}

impl Lifecycle {
    pub(super) fn complete(&mut self, completions: &mut Slab<Completion>, cqe: CqeResult) -> bool {
        use std::mem;

        match mem::replace(self, Lifecycle::Ignored(Box::new(()))) {
            Lifecycle::Pending(..) => {
                // Pending Operations have not submitted to the ring
                // They should not be receiving completions
                unreachable!("Completion for pending Op")
            }

            x @ Lifecycle::Submitted | x @ Lifecycle::Waiting(..) => {
                if io_uring::cqueue::more(cqe.flags) {
                    let mut list = SlabListIndices::new().into_list(completions);
                    list.push(cqe);
                    *self = Lifecycle::CompletionList(list.into_indices());
                } else {
                    *self = Lifecycle::Completed(cqe);
                }
                if let Lifecycle::Waiting(waker) = x {
                    // waker is woken to notify cqe has arrived
                    // Note: Maybe defer calling until cqe with !`more` flag set?
                    waker.wake();
                }
                false
            }

            lifecycle @ Lifecycle::Ignored(..) => {
                if io_uring::cqueue::more(cqe.flags) {
                    // Not yet complete. The Op has been dropped, so we can drop the CQE
                    // but we must keep the lifecycle alive until no more CQE's expected
                    *self = lifecycle;
                    false
                } else {
                    // This Op has completed, we can drop
                    true
                }
            }

            Lifecycle::Completed(..) => {
                // Completions with more flag set go straight onto the slab,
                // and are handled in Lifecycle::CompletionList.
                // To construct Lifecycle::Completed, a CQE with `more` flag unset was received
                // we shouldn't be receiving another.
                unreachable!("invalid operation state")
            }

            Lifecycle::CompletionList(indices) => {
                // A completion list may contain CQE's with and without `more` flag set.
                // Only the final one may have `more` unset, although we don't check.
                let mut list = indices.into_list(completions);
                list.push(cqe);
                *self = Lifecycle::CompletionList(list.into_indices());
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

        fn complete(self, cqe: CqeResult) -> Self::Output {
            Completion {
                result: cqe.result,
                flags: cqe.flags,
                data: self.clone(),
            }
        }
    }

    #[test]
    fn op_stays_in_slab_on_drop() {
        let (op, data) = init();
        drop(op);

        assert_eq!(2, Rc::strong_count(&data));

        assert_eq!(1, num_operations());
        release();
    }

    #[test]
    fn poll_op_once() {
        let (op, data) = init();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());
        assert_eq!(2, Rc::strong_count(&data));

        complete(&op, Ok(1));
        assert_eq!(1, num_operations());
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
        assert_eq!(0, num_operations());

        release();
    }

    #[test]
    fn poll_op_twice() {
        {
            let (op, ..) = init();
            let mut op = task::spawn(op);
            assert_pending!(op.poll());
            assert_pending!(op.poll());

            complete(&op, Ok(1));

            assert!(op.is_woken());
            let Completion { result, flags, .. } = assert_ready!(op.poll());
            assert_eq!(1, result.unwrap());
            assert_eq!(0, flags);
        }

        release();
    }

    #[test]
    fn poll_change_task() {
        {
            let (op, ..) = init();
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
        }

        release();
    }

    #[test]
    fn complete_before_poll() {
        let (op, data) = init();
        let mut op = task::spawn(op);
        complete(&op, Ok(1));
        assert_eq!(1, num_operations());
        assert_eq!(2, Rc::strong_count(&data));

        let Completion { result, flags, .. } = assert_ready!(op.poll());
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        drop(op);
        assert_eq!(0, num_operations());

        release();
    }

    #[test]
    fn complete_after_drop() {
        let (op, data) = init();
        let index = op.index;
        drop(op);

        assert_eq!(2, Rc::strong_count(&data));

        assert_eq!(1, num_operations());

        let cqe = CqeResult {
            result: Ok(1),
            flags: 0,
        };

        CONTEXT.with(|cx| cx.with_driver_mut(|driver| driver.ops.complete(index, cqe)));

        assert_eq!(1, Rc::strong_count(&data));
        assert_eq!(0, num_operations());

        release();
    }

    /// Create a data, and an Op containing that data,
    /// which is mocked as submitted on the ring
    fn init() -> (Op<Rc<()>>, Rc<()>) {
        use crate::driver::Driver;

        let driver = Driver::new(&crate::builder()).unwrap();
        let data = Rc::new(());

        let op = CONTEXT.with(|cx| {
            cx.set_driver(driver);
            cx.with_driver_mut(|driver| {
                let entry = driver.ops.insert();
                let op = Op::new(data.clone(), entry.key());
                entry.insert(Lifecycle::Submitted);
                op
            })
        });
        (op, data)
    }

    fn num_operations() -> usize {
        CONTEXT.with(|cx| cx.with_driver_mut(|driver| driver.num_operations()))
    }

    fn complete(op: &Op<Rc<()>>, result: io::Result<u32>) {
        let cqe = CqeResult { result, flags: 0 };
        CONTEXT.with(|cx| cx.with_driver_mut(|driver| driver.ops.complete(op.index, cqe)));
    }

    fn release() {
        CONTEXT.with(|cx| {
            cx.with_driver_mut(|driver| {
                driver.ops.lifecycle.clear();
                driver.ops.completions.clear();
            });

            cx.unset_driver();
        });
    }
}
