use crate::{
    buf::{bufgroup::BufX, bufring::BufRing},
    io::SharedFd,
    OneshotOutputTransform, UnsubmittedOneshot,
};
use io_uring::{cqueue::Entry, squeue};
use std::io;

/// An unsubmitted recv_provbuf operation.
pub type UnsubmittedRecvProvBuf = UnsubmittedOneshot<RecvProvBufData, RecvProvBufTransform>;

#[allow(missing_docs)]
pub struct RecvProvBufData {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    /// The bufgroup that supplies the bgid and the get_buf function.
    group: BufRing,
}

#[allow(missing_docs)]
pub struct RecvProvBufTransform {
    // _phantom: PhantomData,
}

impl OneshotOutputTransform for RecvProvBufTransform {
    type Output = Result<Option<BufX>, io::Error>;
    type StoredData = RecvProvBufData;

    fn transform_oneshot_output(self, data: Self::StoredData, cqe: Entry) -> Self::Output {
        let res = if cqe.result() >= 0 {
            cqe.result() as u32
        } else {
            return Err(io::Error::from_raw_os_error(-cqe.result()));
        };
        let flags = cqe.flags();

        // Safety: getting a buffer from the group requires the res and flags values accurately
        // identify a buffer and the length which was written to by the kernel. The res and flags
        // passed here are those provided by the kernel.
        unsafe { data.group.get_buf(res, flags) }
    }
}

impl UnsubmittedRecvProvBuf {
    pub(crate) fn recv_provbuf(fd: &SharedFd, group: BufRing, flags: Option<i32>) -> Self {
        use io_uring::{opcode, types};

        let bgid = group.bgid();

        Self::new(
            RecvProvBufData {
                _fd: fd.clone(),
                group,
            },
            RecvProvBufTransform {
                // _phantom: PhantomData::default(),
            },
            opcode::Recv::new(types::Fd(fd.raw_fd()), std::ptr::null_mut(), 0 as _)
                .flags(flags.unwrap_or(0))
                .buf_group(bgid)
                .build()
                .flags(squeue::Flags::BUFFER_SELECT),
        )
    }
}
