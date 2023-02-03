use crate::io::Close;
use std::future::poll_fn;

use std::cell::RefCell;
use std::os::unix::io::{FromRawFd, RawFd};
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
    fd: Fd,

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
}

impl SharedFd {
    pub(crate) fn new(fd: RawFd) -> SharedFd {
        SharedFd {
            inner: Rc::new(Inner {
                fd: Fd(fd),
                state: RefCell::new(State::Init),
            }),
        }
    }

    /// Returns the RawFd
    pub(crate) fn raw_fd(&self) -> RawFd {
        self.inner.fd.0
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
            Ok(true) => match Op::close(self.fd.0) {
                Ok(op) => State::Closing(op),
                Err(_) => {
                    let _ = unsafe { std::fs::File::from_raw_fd(self.fd.0) };
                    State::Closed
                }
            },
            _ => {
                let _ = unsafe { std::fs::File::from_raw_fd(self.fd.0) };
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

// TODO maybe find a better file for this later.

/// A file descriptor that has not been registered with io_uring.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Fd(pub RawFd); // TODO consider renaming to RawFd

/// A file descriptor that has been registered with io_uring using
/// [`Submitter::register_files`](crate::Submitter::register_files) or [`Submitter::register_files_sparse`](crate::Submitter::register_files_sparse).
/// This can reduce overhead compared to using [`Fd`] in some cases.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)] // TODO this probably isn't significant for this crate
pub struct Fixed(pub u32); // TODO consider renaming to Direct (but uring docs use both Fixed descriptor and Direct descriptor)

pub(crate) mod sealed {
    use super::{Fd, Fixed};
    use std::os::unix::io::RawFd;

    #[derive(Debug)]
    // Note: io-uring crate names this Target
    pub enum CommonFd {
        Fd(RawFd),
        Fixed(u32),
    }

    // Note: io-uring crate names this UseFd
    pub trait UseRawFd: Sized {
        fn into(self) -> RawFd;
    }

    // Note: ioo-uring crate names this UseFixed
    pub trait UseCommonFd: Sized {
        fn into(self) -> CommonFd;
    }

    impl UseRawFd for Fd {
        #[inline]
        fn into(self) -> RawFd {
            self.0
        }
    }

    impl UseCommonFd for Fd {
        #[inline]
        fn into(self) -> CommonFd {
            CommonFd::Fd(self.0)
        }
    }

    impl UseCommonFd for Fixed {
        #[inline]
        fn into(self) -> CommonFd {
            CommonFd::Fixed(self.0)
        }
    }
}
