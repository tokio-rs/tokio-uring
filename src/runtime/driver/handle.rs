//! Internal, reference-counted handle to the driver.
//!
//! The driver was previously managed exclusively by thread-local context, but this proved
//! untenable.
//!
//! The new system uses a handle which reference-counts the driver to track ownership and access to
//! the driver.
//!
//! There are two handles.
//! The strong handle is owning, and the weak handle is non-owning.
//! This is important for avoiding reference cycles.
//! The weak handle should be used by anything which is stored in the driver or does not need to
//! keep the driver alive for it's duration.

use io_uring::{cqueue, squeue};
use std::cell::RefCell;
use std::io;
use std::ops::Deref;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::{Rc, Weak};
use std::task::{Context, Poll};

use crate::buf::fixed::FixedBuffers;
use crate::runtime::driver::op::{Completable, MultiCQEFuture, Op, Updateable};
use crate::runtime::driver::Driver;

#[derive(Clone)]
pub(crate) struct Handle {
    pub(super) inner: Rc<RefCell<Driver>>,
}

#[derive(Clone)]
pub(crate) struct WeakHandle {
    inner: Weak<RefCell<Driver>>,
}

impl Handle {
    pub(crate) fn new(b: &crate::Builder) -> io::Result<Self> {
        Ok(Self {
            inner: Rc::new(RefCell::new(Driver::new(b)?)),
        })
    }

    pub(crate) fn dispatch_completions(&self) {
        self.inner.borrow_mut().dispatch_completions()
    }

    pub(crate) fn flush(&self) -> io::Result<usize> {
        self.inner.borrow_mut().uring.submit()
    }

    pub(crate) fn register_buffers(
        &self,
        buffers: Rc<RefCell<dyn FixedBuffers>>,
    ) -> io::Result<()> {
        self.inner.borrow_mut().register_buffers(buffers)
    }

    pub(crate) fn unregister_buffers(
        &self,
        buffers: Rc<RefCell<dyn FixedBuffers>>,
    ) -> io::Result<()> {
        self.inner.borrow_mut().unregister_buffers(buffers)
    }

    pub(crate) fn submit_op_2(&self, sqe: squeue::Entry) -> usize {
        self.inner.borrow_mut().submit_op_2(sqe)
    }

    pub(crate) fn submit_op<T, S, F>(&self, data: T, f: F) -> io::Result<Op<T, S>>
    where
        T: Completable,
        F: FnOnce(&mut T) -> squeue::Entry,
    {
        self.inner.borrow_mut().submit_op(data, f, self.into())
    }

    pub(crate) fn poll_op<T>(&self, op: &mut Op<T>, cx: &mut Context<'_>) -> Poll<T::Output>
    where
        T: Unpin + 'static + Completable,
    {
        self.inner.borrow_mut().poll_op(op, cx)
    }

    pub(crate) fn poll_op_2(&self, index: usize, cx: &mut Context<'_>) -> Poll<cqueue::Entry> {
        self.inner.borrow_mut().poll_op_2(index, cx)
    }

    pub(crate) fn poll_multishot_op<T>(
        &self,
        op: &mut Op<T, MultiCQEFuture>,
        cx: &mut Context<'_>,
    ) -> Poll<T::Output>
    where
        T: Unpin + 'static + Completable + Updateable,
    {
        self.inner.borrow_mut().poll_multishot_op(op, cx)
    }

    pub(crate) fn remove_op<T, CqeType>(&self, op: &mut Op<T, CqeType>) {
        self.inner.borrow_mut().remove_op(op)
    }

    pub(crate) fn remove_op_2<T: 'static>(&self, index: usize, data: T) {
        self.inner.borrow_mut().remove_op_2(index, data)
    }
}

impl WeakHandle {
    pub(crate) fn upgrade(&self) -> Option<Handle> {
        Some(Handle {
            inner: self.inner.upgrade()?,
        })
    }
}

impl AsRawFd for Handle {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.borrow().uring.as_raw_fd()
    }
}

impl From<Driver> for Handle {
    fn from(driver: Driver) -> Self {
        Self {
            inner: Rc::new(RefCell::new(driver)),
        }
    }
}

impl<T> From<T> for WeakHandle
where
    T: Deref<Target = Handle>,
{
    fn from(handle: T) -> Self {
        Self {
            inner: Rc::downgrade(&handle.inner),
        }
    }
}
