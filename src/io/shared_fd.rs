use crate::io::Close;
use std::future::poll_fn;

use std::cell::RefCell;
use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
use std::rc::Rc;
use std::task::Waker;

use crate::runtime::driver::op::Op;
use crate::runtime::CONTEXT;

// Tracks in-flight operations on a file descriptor. Ensures all in-flight
// operations complete before submitting the close.
//
// If the runtime is unavailable, will fall back to synchronous Close to ensure
// File resources are not leaked.
#[derive(Clone)]
pub(crate) struct SharedFd {
    inner: Rc<Inner>,
}

struct Inner {
    // Open file descriptor
    fd: RawFd,

    // Waker to notify when the close operation completes.
    state: RefCell<State>,
}

enum State {
    /// Initial state
    Init,

    /// Waiting for all in-flight operation to complete.
    Waiting(Option<Waker>),

    /// The FD is closing
    Closing(Op<Close>),

    /// The FD is fully closed
    Closed,

    // The FD is leaked. A SharedFd in this state will not be closed on drop.
    // A SharedFd will only enter this state as a result of the `into_raw_fd`
    // method.
    Leaked,
}

impl IntoRawFd for SharedFd {
    // Consumes this object, returning the raw underlying file descriptor.
    // This method will panic if there are any in-flight operations.
    fn into_raw_fd(mut self) -> RawFd {
        // Change the SharedFd state to `Leaked` so that the file-descriptor is
        // not closed on drop.
        if let Some(inner) = Rc::get_mut(&mut self.inner) {
            let state = RefCell::get_mut(&mut inner.state);
            *state = State::Leaked;
            return self.raw_fd();
        }
        panic!("unexpected operation in-flight")
    }
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

    /// An FD cannot be closed until all in-flight operation have completed.
    /// This prevents bugs where in-flight reads could operate on the incorrect
    /// file descriptor.
    ///
    /// TO model this, if there are no in-flight operations, then
    pub(crate) async fn close(mut self) {
        // Get a mutable reference to Inner, indicating there are no
        // in-flight operations on the FD.
        if let Some(inner) = Rc::get_mut(&mut self.inner) {
            // Submit the close operation
            inner.submit_close_op();
        }

        self.inner.closed().await;
    }
}

impl Inner {
    /// If there are no in-flight operations, submit the operation.
    fn submit_close_op(&mut self) {
        // Close the FD
        let state = RefCell::get_mut(&mut self.state);

        // Submit a close operation
        // If either:
        //  - runtime has already closed, or
        //  - submitting the Close operation fails
        // we fall back on a synchronous `close`. This is safe as, at this point,
        // we guarantee all in-flight operations have completed. The most
        // common cause for an error is attempting to close the FD while
        // off runtime.
        //
        // This is done by initializing a `File` with the FD and
        // dropping it.
        //
        // TODO: Should we warn?
        *state = match CONTEXT.try_with(|cx| cx.is_set()) {
            Ok(true) => match Op::close(self.fd) {
                Ok(op) => State::Closing(op),
                Err(_) => {
                    let _ = unsafe { std::fs::File::from_raw_fd(self.fd) };
                    State::Closed
                }
            },
            _ => {
                let _ = unsafe { std::fs::File::from_raw_fd(self.fd) };
                State::Closed
            }
        };
    }

    /// Completes when the FD has been closed.
    async fn closed(&self) {
        use std::future::Future;
        use std::pin::Pin;
        use std::task::Poll;

        poll_fn(|cx| {
            let mut state = self.state.borrow_mut();

            match &mut *state {
                State::Init => {
                    *state = State::Waiting(Some(cx.waker().clone()));
                    Poll::Pending
                }
                State::Waiting(Some(waker)) => {
                    if !waker.will_wake(cx.waker()) {
                        *waker = cx.waker().clone();
                    }

                    Poll::Pending
                }
                State::Waiting(None) => {
                    *state = State::Waiting(Some(cx.waker().clone()));
                    Poll::Pending
                }
                State::Closing(op) => {
                    // Nothing to do if the close opeation failed.
                    let _ = ready!(Pin::new(op).poll(cx));
                    *state = State::Closed;
                    Poll::Ready(())
                }
                State::Closed => Poll::Ready(()),
                // By definition, a SharedFd in a Leaked state is not closed, so we should
                // never encounter this case while waiting for a SharedFd to close.
                State::Leaked => unreachable!(),
            }
        })
        .await;
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Submit the close operation, if needed
        match RefCell::get_mut(&mut self.state) {
            State::Init | State::Waiting(..) => {
                self.submit_close_op();
            }
            _ => {}
        }
    }
}
