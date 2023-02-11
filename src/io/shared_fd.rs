use std::future::poll_fn;

use std::{
    cell::RefCell,
    io,
    os::unix::io::{FromRawFd, RawFd},
    rc::Rc,
    task::Waker,
};

use crate::io::shared_fd::sealed::CommonFd;
use crate::runtime::driver::op::Op;
use crate::runtime::CONTEXT;

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
    fd: CommonFd,

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
        Self::_new(CommonFd::Raw(fd))
    }

    pub(crate) fn new_fixed(slot: u32) -> SharedFd {
        Self::_new(CommonFd::Fixed(slot))
    }

    fn _new(fd: CommonFd) -> SharedFd {
        SharedFd {
            inner: Rc::new(Inner {
                fd,
                state: RefCell::new(State::Init),
            }),
        }
    }

    /*
     * This function name won't make sense when this fixed file feature
     * is fully fleshed out. For now, we panic if called on
     * a fixed file.
     */
    /// Returns the RawFd.
    pub(crate) fn raw_fd(&self) -> RawFd {
        match self.inner.fd {
            CommonFd::Raw(raw) => raw,
            CommonFd::Fixed(_fixed) => {
                // TODO remove this function completely once all the uring opcodes that accept
                // a fixed file table slot have been modified. For now, we have to keep it to avoid
                // too many file changes all at once.
                unreachable!("fixed file support not yet added for this call stack");
            }
        }
    }

    // Returns the common fd, either a RawFd or the fixed fd slot number.
    #[allow(dead_code)]
    pub(crate) fn common_fd(&self) -> CommonFd {
        self.inner.fd
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
                        *waker = cx.waker().clone();
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
        // If the inner state isn't `Closed`, the user hasn't called close().await so close it now.
        // At least for the case of a regular file descriptor we can do it synchronously. For the
        // case of a fixed file table descriptor, we may already be out of the driver's context,
        // but if we aren't we resort to the io_uring close operation - and spawn a task to do it.

        let state = self.state.borrow_mut();

        if let State::Closed = *state {
            return;
        }

        // Perform one form of close or the other.
        match self.fd {
            CommonFd::Raw(raw) => {
                let _ = unsafe { std::fs::File::from_raw_fd(raw) };
            }

            CommonFd::Fixed(fixed) => {
                // As there is no synchronous close for a fixed file table slot, we have to resort
                // to the async close provided by the io_uring device. If we knew the fixed file
                // table had been unregistered, this wouldn't be necessary either.

                match CONTEXT.try_with(|cx| cx.is_set()) {
                    Ok(true) => {}
                    // If the driver is gone, nothing to do. The fixed table has already been taken
                    // down by the device anyway.
                    _ => return,
                }

                crate::spawn(async move {
                    if let Ok(true) = CONTEXT.try_with(|cx| cx.is_set()) {
                        let fd = CommonFd::Fixed(fixed);
                        if let Ok(op) = Op::close(fd) {
                            let _ = op.await;
                        }
                        // Else, should warn or panic if the Op::Close can't be built? It would
                        // mean the fixed value was out of reach which would not be expected at
                        // this point.
                    }
                });
            }
        }
    }
}

// Enum and traits copied from the io-uring crate.

/*
 * TODO maybe these types aren't needed at all.
/// A file descriptor that has not been registered with io_uring.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Raw(pub RawFd); // Note: io-uring names this Fd

/// A file descriptor that has been registered with io_uring using
/// [`Submitter::register_files`](crate::Submitter::register_files) or [`Submitter::register_files_sparse`](crate::Submitter::register_files_sparse).
/// This can reduce overhead compared to using [`Fd`] in some cases.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fixed(pub u32); // TODO consider renaming to Direct (but uring docs use both Fixed descriptor and Direct descriptor)
 */

// TODO definitely not sure this should be sealed. But leaving it for now. Could easily decide
// there is nothing here to seal as our API is fluid for a while yet.

pub(crate) mod sealed {
    // use super::{Fixed, Raw};
    use std::os::unix::io::RawFd;

    #[derive(Debug, Clone, Copy)]
    // Note: io-uring names this Target
    pub(crate) enum CommonFd {
        Raw(RawFd),
        Fixed(u32),
    }

    // Note: io-uring names this UseFd
    pub(crate) trait UseRawFd: Sized {
        fn into(self) -> RawFd;
    }

    // Note: io-uring names this UseFixed
    pub(crate) trait UseCommonFd: Sized {
        fn into(self) -> CommonFd;
    }

    /* TODO maybe won't be needed at all.
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
    */
}
