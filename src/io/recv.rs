use crate::{
    buf::BoundedBufMut, io::SharedFd, BufResult, OneshotOutputTransform, UnsubmittedOneshot,
};
use io_uring::cqueue::Entry;
use std::io;
use std::marker::PhantomData;

/// An unsubmitted recv operation.
pub type UnsubmittedRecv<T> = UnsubmittedOneshot<RecvData<T>, RecvTransform<T>>;

#[allow(missing_docs)]
pub struct RecvData<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    buf: T,
}

#[allow(missing_docs)]
pub struct RecvTransform<T> {
    _phantom: PhantomData<T>,
}

impl<T> OneshotOutputTransform for RecvTransform<T> {
    type Output = BufResult<usize, T>;
    type StoredData = RecvData<T>;

    fn transform_oneshot_output(self, data: Self::StoredData, cqe: Entry) -> Self::Output {
        let res = if cqe.result() >= 0 {
            Ok(cqe.result() as usize)
        } else {
            Err(io::Error::from_raw_os_error(-cqe.result()))
        };

        (res, data.buf)
    }
}

impl<T: BoundedBufMut> UnsubmittedRecv<T> {
    pub(crate) fn recv(fd: &SharedFd, mut buf: T, flags: Option<i32>) -> Self {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.stable_mut_ptr();
        let len = buf.bytes_init();

        Self::new(
            RecvData {
                _fd: fd.clone(),
                buf,
            },
            RecvTransform {
                _phantom: PhantomData::default(),
            },
            opcode::Recv::new(types::Fd(fd.raw_fd()), ptr, len as _)
                .flags(flags.unwrap_or(0))
                .build(),
        )
    }
}
