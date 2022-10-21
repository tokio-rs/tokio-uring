use crate::{
    bufgroup::{self, BufX},
    driver::{op::Completable, Op, SharedFd},
};

use io_uring::squeue;
use std::io;

pub(crate) struct ReadBg<G: Clone + bufgroup::Group> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// The buf_group is kept while the read operation is live. When a buffer has been identified
    /// by the cqe result, the buf_group is handed off to the BufX that is returned.
    ///
    /// If the read fails, there is no buffer chosen by the kernel and the buf_group is allowed to
    /// go out of scope when the ReadBg goes out of scope.
    buf_group: Option<G>,
}

impl<G> Op<ReadBg<G>>
where
    G: Clone + bufgroup::Group + std::marker::Unpin + 'static,
{
    pub(crate) fn read_at_bg(fd: &SharedFd, offset: u64, buf_group: G) -> io::Result<Self> {
        use io_uring::{opcode, types};

        Op::submit_with(
            ReadBg {
                fd: fd.clone(),
                buf_group: Some(buf_group), // is taken below
            },
            |readbg| {
                match &readbg.buf_group {
                    Some(buf_group) => {
                        // Get raw buffer info
                        opcode::Read::new(types::Fd(fd.raw_fd()), std::ptr::null_mut(), 0)
                            .offset(offset as _)
                            .buf_group(buf_group.bgid())
                            .build()
                            .flags(squeue::Flags::BUFFER_SELECT)
                    }
                    None => panic!("not reachable"),
                }
            },
        )
    }
}

impl<G> Completable for ReadBg<G>
where
    G: Clone + bufgroup::Group + std::marker::Unpin + 'static,
{
    type Output = BufX<G>;

    fn complete(mut self, result: io::Result<u32>, flags: u32) -> Self::Output {
        let res = result.unwrap();

        // Take ownership of the buf_group so it can be given to the get_buf call.
        let buf_group = self.buf_group.take().unwrap();

        // TODO interesting this complete doesn't allow an error to be returned.
        // The earlier version of the poll did.
        buf_group
            .clone()
            .get_buf(res, flags, buf_group)
            .expect("need to be able to get a buffer from the buf group")
    }
}
