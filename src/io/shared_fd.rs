use std::future::poll_fn;

use std::{
    cell::RefCell,
    io,
    os::unix::io::{BorrowedFd, FromRawFd, RawFd},
    rc::Rc,
    task::Waker,
};

use crate::runtime::driver::op::Op;

// Tracks in-flight operations on a file descriptor. Ensures all in-flight
// operations complete before submitting the close.
//
// When closing the file descriptor because it is going out of scope, a synchronous close is
// employed.
//
// The closed state is tracked so close calls after the first are ignored.
// Only the first close call returns the true result of closing the file descriptor.
#[derive(Clone)]
pub(crate) struct SharedFd {
    inner: Rc<Inner>,
}

struct Inner {
    // Open file descriptor
    fd: RawFd,

    // Track the sharing state of the file descriptor:
    // normal, being waited on to allow a close by the parent's owner, or already closed.
    state: RefCell<State>,
}

enum State {
    /// Initial state
    Init,

    /// Waiting for the number of strong Rc pointers to drop to 1.
    WaitingForUniqueness(Waker),

    /// The close has been triggered by the parent owner.
    Closed,
}

impl SharedFd {
    pub(crate) fn new(fd: RawFd) -> SharedFd {
        SharedFd {
            inner: Rc::new(Inner {
                fd,
                state: RefCell::new(State::Init),
            }),
        }
    }

    /// Returns the RawFd
    pub(crate) fn raw_fd(&self) -> RawFd {
        self.inner.fd
    }

    pub(crate) fn fd(&self) -> BorrowedFd {
        // SAFETY: we're ensuring the fd stays open as long as SharedFd is
        // alive.
        unsafe { BorrowedFd::borrow_raw(self.inner.fd) }
    }

    /// An FD cannot be closed until all in-flight operation have completed.
    /// This prevents bugs where in-flight reads could operate on the incorrect
    /// file descriptor.
    ///
    pub(crate) async fn close(&mut self) -> io::Result<()> {
        loop {
            // Get a mutable reference to Inner, indicating there are no
            // in-flight operations on the FD.
            if let Some(inner) = Rc::get_mut(&mut self.inner) {
                // Wait for the close operation.
                return inner.async_close_op().await;
            }

            self.sharedfd_is_unique().await;
        }
    }

    /// Completes when the SharedFd's Inner Rc strong count is 1.
    /// Gets polled any time a SharedFd is dropped.
    async fn sharedfd_is_unique(&self) {
        use std::task::Poll;

        poll_fn(|cx| {
            if Rc::<Inner>::strong_count(&self.inner) == 1 {
                return Poll::Ready(());
            }

            let mut state = self.inner.state.borrow_mut();

            match &mut *state {
                State::Init => {
                    *state = State::WaitingForUniqueness(cx.waker().clone());
                    Poll::Pending
                }
                State::WaitingForUniqueness(waker) => {
                    if !waker.will_wake(cx.waker()) {
                        waker.clone_from(cx.waker());
                    }

                    Poll::Pending
                }
                State::Closed => Poll::Ready(()),
            }
        })
        .await;
    }
}

impl Inner {
    async fn async_close_op(&mut self) -> io::Result<()> {
        // &mut self implies there are no outstanding operations.
        // If state already closed, the user closed multiple times; simply return Ok.
        // Otherwise, set state to closed and then submit and await the uring close operation.
        {
            // Release state guard before await.
            let state = RefCell::get_mut(&mut self.state);

            if let State::Closed = *state {
                return Ok(());
            }

            *state = State::Closed;
        }
        Op::close(self.fd)?.await
    }
}

impl Drop for SharedFd {
    fn drop(&mut self) {
        // If the SharedFd state is Waiting
        // The job of the SharedFd's drop is to possibly wake a task that is waiting for the
        // reference count to go down.
        use std::mem;

        let mut state = self.inner.state.borrow_mut();
        if let State::WaitingForUniqueness(_) = *state {
            let state = &mut *state;
            if let State::WaitingForUniqueness(waker) = mem::replace(state, State::Init) {
                // Wake the task wanting to close this SharedFd and let it try again. If it finds
                // there are no more outstanding clones, it will succeed. Otherwise it will start a new
                // Future, waiting for another SharedFd to be dropped.
                waker.wake()
            }
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // If the inner state isn't `Closed`, the user hasn't called close().await
        // so do it synchronously.

        let state = self.state.borrow_mut();

        if let State::Closed = *state {
            return;
        }
        let _ = unsafe { std::fs::File::from_raw_fd(self.fd) };
    }
}
