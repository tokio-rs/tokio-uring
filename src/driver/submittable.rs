use crate::driver::Op;
use io_uring::squeue;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// An unsubmitted but constructed operation.
///
/// This allows users to construct an operation and add flags to it.
/// It is largely intended to be used as a building block for additional internal APIs, but is
/// currently exposed because it provides useful functionality to end users.
///
/// The user data field of the SQE will be overwritten by the IO driver because it is used by the
/// reactor, so don't expect to make use of it.
pub struct Unsubmitted<D, O, F>
where
    F: FnOnce(io::Result<i32>, u32, D) -> io::Result<O>,
{
    entry: squeue::Entry,
    data: D,
    post_op: F,
}

/// An in-flight operation with a post-op subroutine that will be invoked.
///
/// This is intended to be used as a building block for future APIs, but is exposed because its
/// functionality is useful to end users.
#[must_use]
pub struct Submitted<D, O, F>
where
    D: 'static,
    F: FnOnce(io::Result<i32>, u32, D) -> io::Result<O>,
{
    op: Op<D>,
    post_op: Option<F>,
}

impl<D, O, F> Future for Submitted<D, O, F>
where
    F: FnOnce(io::Result<i32>, u32, D) -> io::Result<O> + Unpin,
    D: Unpin + 'static,
{
    type Output = io::Result<O>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let op = &mut this.op;
        tokio::pin!(op);

        let completion = ready!(op.poll(cx));

        let post_op = this.post_op.take().expect("Polled after completed");

        Poll::Ready(post_op(
            completion.result,
            completion.flags,
            completion.data,
        ))
    }
}

impl<D, O, F> Unsubmitted<D, O, F>
where
    F: FnOnce(io::Result<i32>, u32, D) -> io::Result<O>,
    D: Unpin + 'static,
{
    /// Construct an operation that can be submitted after the fact.
    ///
    /// # Params
    ///
    /// - `entry`: SQE to be submitted to io_uring
    /// - `data`: Data referenced in the SQE, whose lifetime now needs to be tied to the
    /// completion of the operation
    /// - `post_op`: Mapper function to be run on the operation after it completes, which
    /// transforms the result into something more usable.
    ///
    /// # Safety
    /// This function relies on the notion that anything 'borrowed' in the SQE is stored in
    /// `data`, and is also *guaranteed not to move while this operation is in progress*.
    /// This almost always means that things in `data` are going to be heap-allocated.
    pub unsafe fn from_raw(entry: squeue::Entry, data: D, post_op: F) -> Self {
        Self {
            entry,
            data,
            post_op,
        }
    }

    /// Add SQE flags to the operation.
    ///
    /// # Safety
    /// This operation assumes that the invariants of `from_raw` are not violated as a result of
    /// the new flags.
    /// Be careful with things like [`io_uring::squeue::Flags::BUFFER_SELECT`], as you need to
    /// ensure that the presence of certain flags was planned for when this object was
    /// constructed, and the `post_op` and `data` fields provided.
    pub unsafe fn apply_flags(&mut self, flags: squeue::Flags) {
        self.entry = self.entry.clone().flags(flags);
    }

    /// Submit this operation to io_uring, and return a future that completes when the operation
    /// does.
    pub fn submit(self) -> io::Result<Submitted<D, O, F>> {
        let Self {
            entry,
            data,
            post_op,
        } = self;
        let op = Op::submit_with(data, |_| entry)?;

        let post_op = Some(post_op);

        Ok(Submitted { op, post_op })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_ops() {
        crate::start(async {
            let entry = io_uring::opcode::Nop::new().build();

            let unsubmitted =
                unsafe { Unsubmitted::from_raw(entry, (), |r, flags, _data| Ok((r?, flags))) };

            let submitted = unsubmitted.submit().unwrap();

            submitted.await.unwrap();
        })
    }
}
