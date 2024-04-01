use std::future::Future;
use std::io;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use io_uring::squeue::Flags;
use io_uring::{cqueue, squeue};

mod link;
mod slab_list;

pub use link::{Link, LinkedInFlightOneshot};
use slab::Slab;
use slab_list::{SlabListEntry, SlabListIndices};

use crate::runtime::{driver, CONTEXT};

/// A SlabList is used to hold unserved completions.
///
/// This is relevant to multi-completion Operations,
/// which require an unknown number of CQE events to be
/// captured before completion.
pub(crate) type Completion = SlabListEntry<CqeResult>;

/// An unsubmitted oneshot operation.
pub struct UnsubmittedOneshot<D: 'static, T: OneshotOutputTransform<StoredData = D>> {
    stable_data: D,
    post_op: T,
    sqe: squeue::Entry,
}

impl<D, T: OneshotOutputTransform<StoredData = D>> UnsubmittedOneshot<D, T> {
    /// Construct a new operation for later submission.
    pub fn new(stable_data: D, post_op: T, sqe: squeue::Entry) -> Self {
        Self {
            stable_data,
            post_op,
            sqe,
        }
    }

    /// Link two UnsubmittedOneshots.
    pub fn link<D2, T2: OneshotOutputTransform<StoredData = D2>>(
        self,
        other: UnsubmittedOneshot<D2, T2>,
    ) -> Link<UnsubmittedOneshot<D, T>, UnsubmittedOneshot<D2, T2>> {
        Link::new(self.set_flags(Flags::IO_LINK), other)
    }

    /// Hard-link two UnsubmittedOneshots.
    pub fn hard_link<D2, T2: OneshotOutputTransform<StoredData = D2>>(
        self,
        other: UnsubmittedOneshot<D2, T2>,
    ) -> Link<UnsubmittedOneshot<D, T>, UnsubmittedOneshot<D2, T2>> {
        Link::new(self.set_flags(Flags::IO_HARDLINK), other)
    }

    /// Set the SQE's flags.
    pub fn set_flags(mut self, flags: Flags) -> Self {
        self.sqe = self.sqe.flags(flags);
        self
    }
}

impl<D, T: OneshotOutputTransform<StoredData = D>> Submit for UnsubmittedOneshot<D, T> {
    type Output = InFlightOneshot<D, T>;

    /// Submit an operation to the driver for batched entry to the kernel.
    fn submit(self) -> Self::Output {
        let handle = CONTEXT
            .with(|x| x.handle())
            .expect("Could not submit op; not in runtime context");

        let index = handle.submit_op_2(self.sqe);

        let inner = InFlightOneshotInner {
            index,
            driver: (&handle).into(),
            stable_data: self.stable_data,
            post_op: self.post_op,
        };

        InFlightOneshot { inner: Some(inner) }
    }
}

/// An in-progress oneshot operation which can be polled for completion.
pub struct InFlightOneshot<D: 'static, T: OneshotOutputTransform<StoredData = D>> {
    inner: Option<InFlightOneshotInner<D, T>>,
}

struct InFlightOneshotInner<D, T: OneshotOutputTransform<StoredData = D>> {
    driver: driver::WeakHandle,
    index: usize,
    stable_data: D,
    post_op: T,
}

impl<D: Unpin, T: OneshotOutputTransform<StoredData = D> + Unpin> Future for InFlightOneshot<D, T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let inner = this
            .inner
            .as_mut()
            .expect("Cannot poll already-completed operation");

        let index = inner.index;

        let upgraded = inner
            .driver
            .upgrade()
            .expect("Failed to poll op: driver no longer exists");

        let cqe = ready!(upgraded.poll_op_2(index, cx));

        let inner = this.inner.take().unwrap();

        Poll::Ready(
            inner
                .post_op
                .transform_oneshot_output(inner.stable_data, cqe),
        )
    }
}

impl<D: 'static, T: OneshotOutputTransform<StoredData = D>> Drop for InFlightOneshot<D, T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            if let Some(driver) = inner.driver.upgrade() {
                driver.remove_op_2(inner.index, inner.stable_data)
            }
        }
    }
}

/// Submit an operation or operations to the driver.
pub trait Submit {
    /// The output of the submission with an in-flight operation or linked in-flight operations.
    type Output;

    /// Submit an operation or linked operations.
    fn submit(self) -> Self::Output;
}

/// Transforms the output of a oneshot operation into a more user-friendly format.
pub trait OneshotOutputTransform {
    /// The final output after the transformation.
    type Output;
    /// The stored data within the op.
    type StoredData;
    /// Transform the stored data and the cqe into the final output.
    fn transform_oneshot_output(self, data: Self::StoredData, cqe: cqueue::Entry) -> Self::Output;
}

/// In-flight operation
pub(crate) struct Op<T: 'static, CqeType = SingleCQE> {
    driver: driver::WeakHandle,
    // Operation index in the slab
    index: usize,

    // Per-operation data
    data: Option<T>,

    // CqeType marker
    _cqe_type: PhantomData<CqeType>,
}

/// A Marker for Ops which expect only a single completion event
pub(crate) struct SingleCQE;

/// A Marker for Operations will process multiple completion events,
/// which combined resolve to a single Future value
pub(crate) struct MultiCQEFuture;

pub(crate) trait Completable {
    type Output;
    /// `complete` will be called for cqe's do not have the `more` flag set
    fn complete(self, cqe: CqeResult) -> Self::Output;
}

pub(crate) trait Updateable: Completable {
    /// Update will be called for cqe's which have the `more` flag set.
    /// The Op should update any internal state as required.
    fn update(&mut self, cqe: CqeResult);
}

pub(crate) enum Lifecycle {
    /// The operation has been submitted to uring and is currently in-flight
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result. The state
    /// must be passed to the driver and held until the operation completes.
    Ignored(Box<dyn std::any::Any>),

    /// The operation has completed with a single cqe result
    Completed(cqueue::Entry),

    /// One or more completion results have been recieved
    /// This holds the indices uniquely identifying the list within the slab
    CompletionList(SlabListIndices),
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
    pub(super) fn new(driver: driver::WeakHandle, data: T, index: usize) -> Self {
        Op {
            driver,
            index,
            data: Some(data),
            _cqe_type: PhantomData,
        }
    }

    pub(super) fn index(&self) -> usize {
        self.index
    }

    pub(super) fn take_data(&mut self) -> Option<T> {
        self.data.take()
    }

    pub(super) fn insert_data(&mut self, data: T) {
        self.data = Some(data);
    }
}

impl<T> Future for Op<T, SingleCQE>
where
    T: Unpin + 'static + Completable,
{
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.driver
            .upgrade()
            .expect("Not in runtime context")
            .poll_op(self.get_mut(), cx)
    }
}

impl<T> Future for Op<T, MultiCQEFuture>
where
    T: Unpin + 'static + Completable + Updateable,
{
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.driver
            .upgrade()
            .expect("Not in runtime context")
            .poll_multishot_op(self.get_mut(), cx)
    }
}

/// The operation may have pending cqe's not yet processed.
/// To manage this, the lifecycle associated with the Op may if required
/// be placed in LifeCycle::Ignored state to handle cqe's which arrive after
/// the Op has been dropped.
impl<T, CqeType> Drop for Op<T, CqeType> {
    fn drop(&mut self) {
        self.driver
            .upgrade()
            .expect("Not in runtime context")
            .remove_op(self)
    }
}

impl Lifecycle {
    pub(crate) fn complete(
        &mut self,
        completions: &mut Slab<Completion>,
        cqe: cqueue::Entry,
    ) -> bool {
        use std::mem;

        match mem::replace(self, Lifecycle::Submitted) {
            x @ Lifecycle::Submitted | x @ Lifecycle::Waiting(..) => {
                if io_uring::cqueue::more(cqe.flags()) {
                    let mut list = SlabListIndices::new().into_list(completions);
                    list.push(cqe.into());
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
                if io_uring::cqueue::more(cqe.flags()) {
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
                list.push(cqe.into());
                *self = Lifecycle::CompletionList(list.into_indices());
                false
            }
        }
    }
}
