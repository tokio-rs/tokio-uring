use io_uring::cqueue::Entry;

use crate::buf::BoundedBufMut;
use crate::io::SharedFd;
use crate::{BufResult, OneshotOutputTransform, UnsubmittedOneshot};

use std::io;
use std::marker::PhantomData;

/// An unsubmitted read operation.
pub type UnsubmittedRead<T> = UnsubmittedOneshot<ReadData<T>, ReadTransform<T>>;

#[allow(missing_docs)]
pub struct ReadData<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    buf: T,
}

#[allow(missing_docs)]
pub struct ReadTransform<T> {
    _phantom: PhantomData<T>,
}

impl<T> OneshotOutputTransform for ReadTransform<T> {
    type Output = BufResult<usize, T>;
    type StoredData = ReadData<T>;

    fn transform_oneshot_output(self, data: Self::StoredData, cqe: Entry) -> Self::Output {
        let res = if cqe.result() >= 0 {
            Ok(cqe.result() as usize)
        } else {
            Err(io::Error::from_raw_os_error(-cqe.result()))
        };

        (res, data.buf)
    }
}

impl<T: BoundedBufMut> UnsubmittedRead<T> {
    pub(crate) fn read_at(fd: &SharedFd, mut buf: T, offset: u64) -> Self {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.stable_mut_ptr();
        let len = buf.bytes_total();

        Self::new(
            ReadData {
                _fd: fd.clone(),
                buf,
            },
            ReadTransform {
                _phantom: PhantomData,
            },
            opcode::Read::new(types::Fd(fd.raw_fd()), ptr, len as _)
                .offset(offset as _)
                .build(),
        )
    }
}
