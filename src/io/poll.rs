use crate::{io::SharedFd, OneshotOutputTransform, UnsubmittedOneshot};
use io_uring::cqueue::Entry;
use std::io;

/// An unsubmitted poll operation.
///
/// Used to call the IORING_OP_POLL_ADD operation.
///
/// The libc POLL flag bits are defined
///
/// libc::{
///     POLLIN,
///     POLLPRI,
///     POLLOUT,
///     POLLERR,
///     POLLHUP,
///     POLLNVAL,
///     POLLRDNORM,
///     POLLRDBAND,
///     }
///
/// #[cfg(not(any(target_arch = "sparc", target_arch = "sparc64")))]
/// libc::POLLRDHUP
/// #[cfg(any(target_arch = "sparc", target_arch = "sparc64"))]
/// libc::POLLRDHUP
pub type UnsubmittedPoll = UnsubmittedOneshot<PollData, PollTransform>;

#[allow(missing_docs)]
pub struct PollData {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,
}

#[allow(missing_docs)]
pub struct PollTransform {}

impl OneshotOutputTransform for PollTransform {
    type Output = io::Result<u32>;
    type StoredData = PollData;

    fn transform_oneshot_output(self, _data: Self::StoredData, cqe: Entry) -> Self::Output {
        if cqe.result() >= 0 {
            Ok(cqe.result() as u32)
        } else {
            Err(io::Error::from_raw_os_error(-cqe.result()))
        }
    }
}

impl UnsubmittedPoll {
    #[allow(dead_code)]
    pub(crate) fn poll(fd: &SharedFd, flags: u32) -> Self {
        use io_uring::{opcode, types};

        Self::new(
            PollData { _fd: fd.clone() },
            PollTransform {},
            opcode::PollAdd::new(types::Fd(fd.raw_fd()), flags as _).build(),
        )
    }
}
