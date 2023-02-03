use crate::io::Close;
use std::future::poll_fn;

use std::cell::RefCell;
use std::os::unix::io::{FromRawFd, RawFd};
use std::rc::Rc;
use std::task::Waker;

use crate::io::shared_fd::sealed::CommonFd;
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
    fd: CommonFd,

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
                fd: CommonFd::Raw(fd),
                state: RefCell::new(State::Init),
            }),
        }
    }
    // TODO once we implement a request that creates a fixed file descriptor, remove this 'allow'.
    // It would be possible to create a fixed file using a `register` command to store a raw fd
    // into the fixed table, but that's a whole other can of worms - do we track both, can either
    // be closed while the other remains active and functional?
    #[allow(dead_code)]
    pub(crate) fn new_fixed(slot: u32) -> SharedFd {
        SharedFd {
            inner: Rc::new(Inner {
                fd: CommonFd::Fixed(slot),
                state: RefCell::new(State::Init),
            }),
        }
    }

    /// Returns the RawFd.
    pub(crate) fn raw_fd(&self) -> RawFd {
        self.inner.raw_fd()
    }

    /// Returns true if self represents a RawFd.
    #[allow(dead_code)]
    pub(crate) fn is_raw_fd(&self) -> bool {
        self.inner.is_raw_fd()
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
    /// Returns the RawFd
    pub(crate) fn raw_fd(&self) -> RawFd {
        //self.inner.fd.0
        match self.fd {
            CommonFd::Raw(raw) => raw,
            CommonFd::Fixed(_fixed) => {
                unreachable!(); // caller could have used is_raw_fd
            }
        }
    }

    /// Returns true if self represents a RawFd.
    pub(crate) fn is_raw_fd(&self) -> bool {
        match self.fd {
            CommonFd::Raw(_) => true,
            CommonFd::Fixed(_) => false,
        }
    }
    /// If there are no in-flight operations, submit the operation.
    fn submit_close_op(&mut self) {
        // Close the file
        let common_fd = self.fd;
        let state = RefCell::get_mut(&mut self.state);

        match *state {
            State::Closing(_) | State::Closed => return,
            _ => {}
        };

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
        if let Ok(true) = CONTEXT.try_with(|cx| cx.is_set()) {
            if let Ok(op) = Op::close(common_fd) {
                *state = State::Closing(op);
                return;
            }
        };

        if let CommonFd::Raw(raw) = common_fd {
            let _ = unsafe { std::fs::File::from_raw_fd(raw) };
        }
        *state = State::Closed;
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
                    // Nothing to do if the close operation failed.
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

// Enum and traits copied from the io-uring crate.

/// A file descriptor that has not been registered with io_uring.
#[derive(Debug, Clone, Copy)]
pub struct Raw(pub RawFd); // Note: io-uring names this Fd

/// A file descriptor that has been registered with io_uring using
/// [`Submitter::register_files`](crate::Submitter::register_files) or [`Submitter::register_files_sparse`](crate::Submitter::register_files_sparse).
/// This can reduce overhead compared to using [`Fd`] in some cases.
#[derive(Debug, Clone, Copy)]
pub struct Fixed(pub u32); // TODO consider renaming to Direct (but uring docs use both Fixed descriptor and Direct descriptor)

pub(crate) mod sealed {
    use super::{Fixed, Raw};
    use std::os::unix::io::RawFd;

    #[derive(Debug, Clone, Copy)]
    // Note: io-uring names this Target
    pub enum CommonFd {
        Raw(RawFd),
        Fixed(u32),
    }

    // Note: io-uring names this UseFd
    pub trait UseRawFd: Sized {
        fn into(self) -> RawFd;
    }

    // Note: io-uring names this UseFixed
    pub trait UseCommonFd: Sized {
        fn into(self) -> CommonFd;
    }

    impl UseRawFd for Raw {
        #[inline]
        fn into(self) -> RawFd {
            self.0
        }
    }

    impl UseCommonFd for Raw {
        #[inline]
        fn into(self) -> CommonFd {
            CommonFd::Raw(self.0)
        }
    }

    impl UseCommonFd for Fixed {
        #[inline]
        fn into(self) -> CommonFd {
            CommonFd::Fixed(self.0)
        }
    }
}
