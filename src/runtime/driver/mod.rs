use crate::buf::fixed::FixedBuffers;
use crate::runtime::driver::op::Lifecycle;
use io_uring::opcode::AsyncCancel;
use io_uring::IoUring;
use slab::Slab;
use std::cell::RefCell;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;

pub(crate) use handle::*;

mod handle;
pub(crate) mod op;

pub(crate) struct Driver {
    /// In-flight operations
    ops: Ops,

    /// IoUring bindings
    pub(crate) uring: IoUring,

    /// Reference to the currently registered buffers.
    /// Ensures that the buffers are not dropped until
    /// after the io-uring runtime has terminated.
    pub(crate) fixed_buffers: Option<Rc<RefCell<dyn FixedBuffers>>>,
}

struct Ops {
    // When dropping the driver, all in-flight operations must have completed. This
    // type wraps the slab and ensures that, on drop, the slab is empty.
    lifecycle: Slab<op::Lifecycle>,

    /// Received but unserviced Op completions
    completions: Slab<op::Completion>,
}

impl Driver {
    pub(crate) fn new(b: &crate::Builder) -> io::Result<Driver> {
        let uring = b.urb.build(b.entries)?;

        Ok(Driver {
            ops: Ops::new(),
            uring,
            fixed_buffers: None,
        })
    }

    fn wait(&self) -> io::Result<usize> {
        self.uring.submit_and_wait(1)
    }

    // only used in tests rn
    #[allow(unused)]
    pub(super) fn num_operations(&self) -> usize {
        self.ops.lifecycle.len()
    }

    pub(crate) fn tick(&mut self) {
        let mut cq = self.uring.completion();
        cq.sync();

        for cqe in cq {
            if cqe.user_data() == u64::MAX {
                // Result of the cancellation action. There isn't anything we
                // need to do here. We must wait for the CQE for the operation
                // that was canceled.
                continue;
            }

            let index = cqe.user_data() as _;

            self.ops.complete(index, cqe.into());
        }
    }

    pub(crate) fn submit(&mut self) -> io::Result<()> {
        loop {
            match self.uring.submit() {
                Ok(_) => {
                    self.uring.submission().sync();
                    return Ok(());
                }
                Err(ref e) if e.raw_os_error() == Some(libc::EBUSY) => {
                    self.tick();
                }
                Err(e) if e.raw_os_error() != Some(libc::EINTR) => {
                    return Err(e);
                }
                _ => continue,
            }
        }
    }
}

impl AsRawFd for Driver {
    fn as_raw_fd(&self) -> RawFd {
        self.uring.as_raw_fd()
    }
}

/// Drop the driver, cancelling any in-progress ops and waiting for them to terminate.
///
/// This first cancels all ops and then waits for them to be moved to the completed lifecycle phase.
///
/// It is possible for this to be run without previously dropping the runtime, but this should only
/// be possible in the case of [`std::process::exit`].
///
/// This depends on us knowing when ops are completed and done firing.
/// When multishot ops are added (support exists but none are implemented), a way to know if such
/// an op is finished MUST be added, otherwise our shutdown process is unsound.
impl Drop for Driver {
    fn drop(&mut self) {
        // get all ops in flight for cancellation
        while !self.uring.submission().is_empty() {
            self.submit().expect("Internal error when dropping driver");
        }

        // Pre-determine what to cancel
        // After this pass, all LifeCycles will be marked either as Completed or Ignored, as appropriate
        for (_, cycle) in self.ops.lifecycle.iter_mut() {
            match std::mem::replace(cycle, Lifecycle::Ignored(Box::new(()))) {
                lc @ Lifecycle::Completed(_) => {
                    // don't cancel completed items
                    *cycle = lc;
                }

                Lifecycle::CompletionList(indices) => {
                    let mut list = indices.clone().into_list(&mut self.ops.completions);
                    if !io_uring::cqueue::more(list.peek_end().unwrap().flags) {
                        // This op is complete. Replace with a null Completed entry
                        *cycle = Lifecycle::Completed(op::CqeResult {
                            result: Ok(0),
                            flags: 0,
                        });
                    }
                }

                _ => {
                    // All other states need cancelling.
                    // The mem::replace means these are now marked Ignored.
                }
            }
        }

        // Submit cancellation for all ops marked Ignored
        for (id, cycle) in self.ops.lifecycle.iter_mut() {
            if let Lifecycle::Ignored(..) = cycle {
                unsafe {
                    while self
                        .uring
                        .submission()
                        .push(&AsyncCancel::new(id as u64).build().user_data(u64::MAX))
                        .is_err()
                    {
                        self.uring
                            .submit_and_wait(1)
                            .expect("Internal error when dropping driver");
                    }
                }
            }
        }

        // Wait until all Lifetimes have been removed from the slab.
        //
        // Ignored entries will be removed from the Lifecycle slab
        // by the complete logic called by `tick()`
        //
        // Completed Entries are removed here directly
        let mut id = 0;
        loop {
            if self.ops.lifecycle.is_empty() {
                break;
            }
            // Cycles are either all ignored or complete
            // If there is at least one Ignored still to process, call wait
            match self.ops.lifecycle.get(id) {
                Some(Lifecycle::Ignored(..)) => {
                    // If waiting fails, ignore the error. The wait will be attempted
                    // again on the next loop.
                    let _ = self.wait();
                    self.tick();
                }

                Some(_) => {
                    // Remove Completed entries
                    let _ = self.ops.lifecycle.remove(id);
                    id += 1;
                }

                None => {
                    id += 1;
                }
            }
        }
    }
}

impl Ops {
    fn new() -> Ops {
        Ops {
            lifecycle: Slab::with_capacity(64),
            completions: Slab::with_capacity(64),
        }
    }

    fn get_mut(&mut self, index: usize) -> Option<(&mut op::Lifecycle, &mut Slab<op::Completion>)> {
        let completions = &mut self.completions;
        self.lifecycle
            .get_mut(index)
            .map(|lifecycle| (lifecycle, completions))
    }

    // Insert a new operation
    fn insert(&mut self) -> usize {
        self.lifecycle.insert(op::Lifecycle::Submitted)
    }

    // Remove an operation
    fn remove(&mut self, index: usize) {
        self.lifecycle.remove(index);
    }

    fn complete(&mut self, index: usize, cqe: op::CqeResult) {
        let completions = &mut self.completions;
        if self.lifecycle[index].complete(completions, cqe) {
            self.lifecycle.remove(index);
        }
    }
}

impl Drop for Ops {
    fn drop(&mut self) {
        assert!(self
            .lifecycle
            .iter()
            .all(|(_, cycle)| matches!(cycle, Lifecycle::Completed(_))))
    }
}

#[cfg(test)]
mod test {
    use std::rc::Rc;

    use crate::runtime::driver::op::{Completable, CqeResult, Op};
    use crate::runtime::CONTEXT;
    use tokio_test::{assert_pending, assert_ready, task};

    use super::*;

    #[derive(Debug)]
    pub(crate) struct Completion {
        result: io::Result<u32>,
        flags: u32,
        data: Rc<()>,
    }

    impl Completable for Rc<()> {
        type Output = Completion;

        fn complete(self, cqe: CqeResult) -> Self::Output {
            Completion {
                result: cqe.result,
                flags: cqe.flags,
                data: self.clone(),
            }
        }
    }

    #[test]
    fn op_stays_in_slab_on_drop() {
        let (op, data) = init();
        drop(op);

        assert_eq!(2, Rc::strong_count(&data));

        assert_eq!(1, num_operations());
        release();
    }

    #[test]
    fn poll_op_once() {
        let (op, data) = init();
        let mut op = task::spawn(op);
        assert_pending!(op.poll());
        assert_eq!(2, Rc::strong_count(&data));

        complete(&op, Ok(1));
        assert_eq!(1, num_operations());
        assert_eq!(2, Rc::strong_count(&data));

        assert!(op.is_woken());
        let Completion {
            result,
            flags,
            data: d,
        } = assert_ready!(op.poll());
        assert_eq!(2, Rc::strong_count(&data));
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        drop(d);
        assert_eq!(1, Rc::strong_count(&data));

        drop(op);
        assert_eq!(0, num_operations());

        release();
    }

    #[test]
    fn poll_op_twice() {
        {
            let (op, ..) = init();
            let mut op = task::spawn(op);
            assert_pending!(op.poll());
            assert_pending!(op.poll());

            complete(&op, Ok(1));

            assert!(op.is_woken());
            let Completion { result, flags, .. } = assert_ready!(op.poll());
            assert_eq!(1, result.unwrap());
            assert_eq!(0, flags);
        }

        release();
    }

    #[test]
    fn poll_change_task() {
        {
            let (op, ..) = init();
            let mut op = task::spawn(op);
            assert_pending!(op.poll());

            let op = op.into_inner();
            let mut op = task::spawn(op);
            assert_pending!(op.poll());

            complete(&op, Ok(1));

            assert!(op.is_woken());
            let Completion { result, flags, .. } = assert_ready!(op.poll());
            assert_eq!(1, result.unwrap());
            assert_eq!(0, flags);
        }

        release();
    }

    #[test]
    fn complete_before_poll() {
        let (op, data) = init();
        let mut op = task::spawn(op);
        complete(&op, Ok(1));
        assert_eq!(1, num_operations());
        assert_eq!(2, Rc::strong_count(&data));

        let Completion { result, flags, .. } = assert_ready!(op.poll());
        assert_eq!(1, result.unwrap());
        assert_eq!(0, flags);

        drop(op);
        assert_eq!(0, num_operations());

        release();
    }

    #[test]
    fn complete_after_drop() {
        let (op, data) = init();
        let index = op.index;
        drop(op);

        assert_eq!(2, Rc::strong_count(&data));

        assert_eq!(1, num_operations());

        let cqe = CqeResult {
            result: Ok(1),
            flags: 0,
        };

        CONTEXT.with(|cx| {
            cx.with_handle_mut(|driver| driver.inner.borrow_mut().ops.complete(index, cqe))
        });

        assert_eq!(1, Rc::strong_count(&data));
        assert_eq!(0, num_operations());

        release();
    }

    fn init() -> (Op<Rc<()>>, Rc<()>) {
        let driver = Driver::new(&crate::builder()).unwrap();
        let data = Rc::new(());

        let op = CONTEXT.with(|cx| {
            cx.set_handle(driver.into());

            cx.with_handle_mut(|driver| {
                let index = driver.inner.borrow_mut().ops.insert();

                Op::new((&*driver).into(), data.clone(), index)
            })
        });

        (op, data)
    }

    fn num_operations() -> usize {
        CONTEXT.with(|cx| cx.with_handle_mut(|driver| driver.inner.borrow().num_operations()))
    }

    fn complete(op: &Op<Rc<()>>, result: io::Result<u32>) {
        let cqe = CqeResult { result, flags: 0 };
        CONTEXT.with(|cx| {
            cx.with_handle_mut(|driver| driver.inner.borrow_mut().ops.complete(op.index, cqe))
        });
    }

    fn release() {
        CONTEXT.with(|cx| {
            cx.with_handle_mut(|driver| {
                driver.inner.borrow_mut().ops.lifecycle.clear();
                driver.inner.borrow_mut().ops.completions.clear();
            });

            cx.unset_driver();
        });
    }
}
