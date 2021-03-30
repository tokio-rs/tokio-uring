use crate::driver::{self, Accept, Close, Open, Read, Write};

use io_uring::{cqueue, squeue};
use std::cell::RefCell;
use std::future::Future;
use std::io;
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// In-flight operation
pub(crate) struct Op<T> {
    // Driver running the operation
    pub(super) driver: Rc<RefCell<driver::Inner>>,

    // Operation index in the slab
    pub(super) index: usize,

    // Tracks the kind of operation
    _p: PhantomData<T>,
}

/// Operation completion. Returns stored state with the result of the operation.
#[derive(Debug)]
pub(crate) struct Completion<T> {
    pub(crate) state: T,
    pub(crate) result: io::Result<u32>,
    pub(crate) flags: u32,
}

pub(crate) struct State {
    /// Kind of operation
    kind: Kind,

    /// Lifecycle of the operation
    lifecycle: Lifecycle,
}

enum Lifecycle {
    /// The operation has been submitted to uring and is currently in-flight
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result.
    Ignored,

    /// The operation has completed.
    Completed(cqueue::Entry),
}

macro_rules! ops {
    (
        $(
            $name:ident,
        )*
    ) => {
        enum Kind {
            $(
                $name($name),
            )*
        }

        $(
            impl From<$name> for State {
                fn from(src: $name) -> State {
                    State {
                        kind: Kind::$name(src),
                        lifecycle: Lifecycle::Submitted,
                    }
                }
            }

            impl From<State> for $name {
                fn from(src: State) -> $name {
                    match src.kind {
                        Kind::$name(val) => val,
                        _ => panic!(),
                    }
                }
            }

            impl AsMut<$name> for State {
                fn as_mut(&mut self) -> &mut $name {
                    match &mut self.kind {
                        Kind::$name(kind) => kind,
                        _ => panic!(),
                    }
                }
            }
        )*
    }
}

ops! {
    Accept,
    Close,
    Open,
    Read,
    Write,
}

impl<T> Op<T> {
    /// Submit an operation to uring.
    ///
    /// `state` is stored during the operation tracking any state submitted to
    /// the kernel.
    pub(super) fn submit_with<F>(state: T, f: F) -> io::Result<Op<T>>
    where
        T: Into<State>,
        State: AsMut<T>,
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        driver::CURRENT.with(|inner_rc| {
            let mut inner = inner_rc.borrow_mut();
            let inner = &mut *inner;

            // Store the operation state
            let index = inner.ops.insert(state.into());

            // Configure the SQE
            let sqe = f(inner.ops[index].as_mut())
                .user_data(index as _);

            let (submitter, mut sq, _) = inner.uring.split();

            if sq.is_full() {
                submitter.submit()?;
                sq.sync();
            }

            if unsafe { sq.push(&sqe).is_err() } {
                unimplemented!("when is this hit?");
            }

            drop(sq);
            submitter.submit()?;

            Ok(Op {
                driver: inner_rc.clone(),
                index,
                _p: PhantomData,
            })
        })
    }
}

impl<T> Future for Op<T>
where
    T: From<State> + Unpin,
{
    type Output = Completion<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::mem;

        let me = &mut *self;
        let mut inner = me.driver.borrow_mut();
        let state = &mut inner.ops[me.index];

        match mem::replace(&mut state.lifecycle, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                state.lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) if !waker.will_wake(cx.waker()) => {
                state.lifecycle = Lifecycle::Waiting(cx.waker().clone());
                Poll::Pending
            }
            Lifecycle::Waiting(waker) => {
                state.lifecycle = Lifecycle::Waiting(waker);
                Poll::Pending
            }
            Lifecycle::Ignored => unreachable!(),
            Lifecycle::Completed(cqe) => {
                let state = inner.ops.remove(me.index);
                me.index = usize::MAX;

                Poll::Ready(Completion {
                    state: state.into(),
                    result: if cqe.result() >= 0 {
                        Ok(cqe.result() as u32)
                    } else {
                        Err(io::Error::from_raw_os_error(-cqe.result()))
                    },
                    flags: cqe.flags(),
                })
            }
        }
    }
}

impl<T> Drop for Op<T> {
    fn drop(&mut self) {
        let mut inner = self.driver.borrow_mut();
        let state = match inner.ops.get_mut(self.index) {
            Some(state) => state,
            None => return,
        };


        match state.lifecycle {
            Lifecycle::Submitted | Lifecycle::Waiting(_) => {
                println!("LOCATION = {:?}", std::panic::Location::caller());
                /*
                TOTAL hax here! shows how to cancel an operation

                // The operation needs to be cancelled
                // TODO: Is this even sane?
                let mut sqe = driver::prepare_sqe(&mut inner.uring).unwrap();
                unsafe {
                    // Configure the cancellation
                    sqe.prep_cancel(self.index as _, 0);

                    // cancellation is tracked as u64::MAX
                    sqe.set_user_data(u64::MAX);
                }

                // TODO: Don't do this every operation!
                inner.uring.submit_sqes().unwrap();

                while let Some(cqe) = inner.uring.peek_for_cqe() {
                    println!("IN CANCEL CQE; data = {}; res = {:?}", cqe.user_data(), cqe.result());
                }
                */

                inner.ops[self.index].lifecycle = Lifecycle::Ignored
            }
            Lifecycle::Completed(_) => {
                inner.ops.remove(self.index);
            }
            Lifecycle::Ignored => unreachable!(),
        }
    }
}

impl State {
    /// Returns true if drop
    pub(super) fn complete(&mut self, cqe: cqueue::Entry) -> bool {
        use std::mem;

        match mem::replace(&mut self.lifecycle, Lifecycle::Submitted) {
            Lifecycle::Submitted => {
                self.lifecycle = Lifecycle::Completed(cqe);
                false

            }
            Lifecycle::Waiting(waker) => {
                self.lifecycle = Lifecycle::Completed(cqe);
                waker.wake();
                false

            }
            Lifecycle::Ignored => {
                true
            }
            Lifecycle::Completed(_) => unreachable!("invalid operation state"),
        }
    }
}