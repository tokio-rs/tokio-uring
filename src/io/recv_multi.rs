use crate::buf::bufgroup::BufX;
use crate::io::SharedFd;

use crate::buf::bufring::BufRing;
use crate::runtime::driver::op::{CqeResult, MultiCQEStream, Op, Streamable};
use crate::runtime::CONTEXT;
use std::io;

use io_uring::squeue;

pub(crate) type RecvMultiStream = Op<RecvMulti, MultiCQEStream>;

pub(crate) struct RecvMulti {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    fd: SharedFd,

    /// The bufgroup that supplies the bgid and the get_buf function.
    group: BufRing,

    flags: Option<i32>,
}

impl RecvMulti {
    fn build_entry(&self) -> squeue::Entry {
        use io_uring::{opcode, types};
        opcode::RecvMulti::new(types::Fd(self.fd.raw_fd()), self.group.bgid())
            .flags(self.flags.unwrap_or(0))
            .build()
    }
}

impl RecvMultiStream {
    pub(crate) fn recv_multi(
        fd: &SharedFd,
        group: BufRing,
        flags: Option<i32>,
    ) -> io::Result<Self> {
        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op_stream(
                    RecvMulti {
                        fd: fd.clone(),
                        group,
                        flags,
                    },
                    |s| s.build_entry(),
                )
        })
    }
}

impl Streamable for RecvMulti {
    type Item = io::Result<BufX>;

    // stream_next is the common case where the multishot operation returns some or many values.
    fn stream_next(&self, cqe: CqeResult) -> Self::Item {
        // The kernel is not expected to provide an error when the `more` bit was set,
        // but it's just as easy to handle the case here.
        let res = cqe.result?;
        let flags = cqe.flags;

        // Safety: getting a buffer from the group requires the res and flags values accurately
        // identify a buffer and the length which was written to by the kernel. The res and flags
        // passed here are those provided by the kernel.
        let get_buf_res = unsafe { self.group.get_buf(res, flags) };

        match get_buf_res {
            Ok(Some(bufx)) => Ok(bufx),
            Ok(None) => {
                unreachable!("unexpected combo of 'more', no error, and no buffer");
            }
            Err(e) => Err(e),
        }
    }

    // stream_complete is the less common case where the kernel has had enough of our operation -
    // typically there is one at the end of the operation because the connection was closed or the
    // kernel ran into an error like there being no buffers available in the pool or the operation
    // was cancelled.
    //
    // But because there are times the kernel terminates the operation for its own expediency
    // reasons, despite there being no error and the connection not being closed, we go the extra
    // mile here and resubmit the request for the user when the cqe indicates a buffer was picked
    // for data. This simplifies the user's logic because they don't have to resubmit the request
    // themselves meaning they don't need an extra loop around their resv_multi call. This actually
    // does what the io_uring documentation says to do ... resubmit the operation to the ring (they
    // make it seem so easy with their 'C' code).
    fn stream_complete(&self, cqe: CqeResult, index: usize) -> Option<(Self::Item, bool)> {
        let res = match cqe.result {
            Ok(res) => res,
            Err(e) => return Some((Err(e), false)),
        };
        let flags = cqe.flags;

        // Safety: getting a buffer from the group requires the res and flags values accurately
        // identify a buffer and the length which was written to by the kernel. The res and flags
        // passed here are those provided by the kernel.
        let get_buf_res = unsafe { self.group.get_buf(res, flags) };

        match get_buf_res {
            Ok(Some(bufx)) => {
                // Use index to resubmit the operation.
                let entry = self.build_entry();
                let resub_result = CONTEXT.with(|x| {
                    x.handle()
                        .expect("Not in a runtime context")
                        .resubmit_op_stream(index, entry)
                });
                match resub_result {
                    // Return true to let the Stream know, in its poll_next_op method, the Op has a
                    // renewed life. It should not be removed from the slab yet.
                    Ok(()) => Some((Ok(bufx), true)),

                    // In the case of an error encountered during resubmission, allow the bufx to
                    // be dropped right here. The resubmission error is more pressing and needs to
                    // be returned.
                    Err(e) => Some((Err(e), false)),
                }
            }
            Ok(None) => None,
            Err(e) => Some((Err(e), false)),
        }
    }
}
