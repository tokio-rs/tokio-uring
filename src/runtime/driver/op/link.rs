use io_uring::squeue::Flags;
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{OneshotOutputTransform, Submit, UnsubmittedOneshot};

/// A Link struct to represent linked operations.
pub struct Link<D, N> {
    data: D,
    next: N,
}

impl<D, N> Link<D, N> {
    /// Construct a new Link with actual data and next node (Link or UnsubmittedOneshot).
    pub fn new(data: D, next: N) -> Self {
        Self { data, next }
    }
}

impl<D, N> Submit for Link<D, N>
where
    D: Submit,
    N: Submit,
{
    type Output = LinkedInFlightOneshot<D::Output, N::Output>;

    fn submit(self) -> Self::Output {
        LinkedInFlightOneshot {
            data: self.data.submit(),
            next: Some(self.next.submit()),
        }
    }
}

impl<D1, T1: OneshotOutputTransform<StoredData = D1>, N> Link<UnsubmittedOneshot<D1, T1>, N> {
    /// Construct a new soft Link with current Link and other UnsubmittedOneshot.
    pub fn link<D2, T2: OneshotOutputTransform<StoredData = D2>>(
        self,
        other: UnsubmittedOneshot<D2, T2>,
    ) -> Link<UnsubmittedOneshot<D1, T1>, Link<N, UnsubmittedOneshot<D2, T2>>> {
        Link {
            data: self.data.set_flags(Flags::IO_LINK),
            next: Link {
                data: self.next,
                next: other,
            },
        }
    }

    /// Construct a new hard Link with current Link and other UnsubmittedOneshot.
    pub fn hard_link<D2, T2: OneshotOutputTransform<StoredData = D2>>(
        self,
        other: UnsubmittedOneshot<D2, T2>,
    ) -> Link<UnsubmittedOneshot<D1, T1>, Link<N, UnsubmittedOneshot<D2, T2>>> {
        Link {
            data: self.data.set_flags(Flags::IO_HARDLINK),
            next: Link {
                data: self.next,
                next: other,
            },
        }
    }
}

pin_project! {
    /// An in-progress linked oneshot operations which can be polled for completion.
    pub struct LinkedInFlightOneshot<D, N> {
        #[pin]
        data: D,
        next: Option<N>,
    }
}

impl<D, N> Future for LinkedInFlightOneshot<D, N>
where
    D: Future,
{
    type Output = (D::Output, N); // Will return actual output and next linked future.

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let output = ready!(this.data.poll(cx));
        let next = this.next.take().unwrap();

        Poll::Ready((output, next))
    }
}
