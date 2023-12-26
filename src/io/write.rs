use crate::{buf::BoundedBuf, io::SharedFd, BufResult, OneshotOutputTransform, UnsubmittedOneshot};
use io_uring::cqueue::Entry;
use io_uring::squeue::Flags;
use std::io;
use std::marker::PhantomData;

/// An unsubmitted write operation.
pub type UnsubmittedWrite<T> = UnsubmittedOneshot<WriteData<T>, WriteTransform<T>>;

#[allow(missing_docs)]
pub struct WriteData<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    _fd: SharedFd,

    buf: T,
}

#[allow(missing_docs)]
pub struct WriteTransform<T> {
    _phantom: PhantomData<T>,
}

impl<T> OneshotOutputTransform for WriteTransform<T> {
    type Output = BufResult<usize, T>;
    type StoredData = WriteData<T>;

    fn transform_oneshot_output(self, data: Self::StoredData, cqe: Entry) -> Self::Output {
        let res = if cqe.result() >= 0 {
            Ok(cqe.result() as usize)
        } else {
            Err(io::Error::from_raw_os_error(-cqe.result()))
        };

        (res, data.buf)
    }
}

impl<T: BoundedBuf> UnsubmittedWrite<T> {
    pub(crate) fn write_at(fd: &SharedFd, buf: T, offset: u64) -> Self {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.stable_ptr();
        let len = buf.bytes_init();

        Self::new(
            WriteData {
                _fd: fd.clone(),
                buf,
            },
            WriteTransform {
                _phantom: PhantomData,
            },
            opcode::Write::new(types::Fd(fd.raw_fd()), ptr, len as _)
                .offset(offset as _)
                .build(),
        )
    }

    pub(crate) fn write_at_with_flags(fd: &SharedFd, buf: T, offset: u64, flags: Flags) -> Self {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.stable_ptr();
        let len = buf.bytes_init();

        Self::new(
            WriteData {
                _fd: fd.clone(),
                buf,
            },
            WriteTransform {
                _phantom: PhantomData,
            },
            opcode::Write::new(types::Fd(fd.raw_fd()), ptr, len as _)
                .offset(offset as _)
                .build()
                .flags(flags),
        )
    }
}
