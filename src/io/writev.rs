use crate::{buf::BoundedBuf, io::SharedFd, BufResult};
use crate::{OneshotOutputTransform, UnsubmittedOneshot};
use io_uring::cqueue::Entry;
use libc::iovec;
use std::io;
use std::marker::PhantomData;

/// An unsubmitted writev operation.
pub type UnsubmittedWritev<T> = UnsubmittedOneshot<WritevData<T>, WritevTransform<T>>;

#[allow(missing_docs)]
pub struct WritevData<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    pub(crate) bufs: Vec<T>,

    /// Parameter for `io_uring::op::readv`, referring `bufs`.
    #[allow(dead_code)]
    iovs: Vec<iovec>,
}

#[allow(missing_docs)]
pub struct WritevTransform<T> {
    _phantom: PhantomData<T>,
}

impl<T> OneshotOutputTransform for WritevTransform<T>
where
    T: BoundedBuf,
{
    type Output = BufResult<usize, Vec<T>>;
    type StoredData = WritevData<T>;

    fn transform_oneshot_output(self, data: Self::StoredData, cqe: Entry) -> Self::Output {
        let res = if cqe.result() >= 0 {
            Ok(cqe.result() as usize)
        } else {
            Err(io::Error::from_raw_os_error(-cqe.result()))
        };

        (res, data.bufs)
    }
}

impl<T: BoundedBuf> UnsubmittedWritev<T> {
    pub(crate) fn writev_at(fd: &SharedFd, mut bufs: Vec<T>, offset: u64) -> Self {
        use io_uring::{opcode, types};

        let iovs: Vec<iovec> = bufs
            .iter_mut()
            .map(|b| iovec {
                // Safety guaranteed by `BoundedBufMut`.
                iov_base: b.stable_ptr() as *mut libc::c_void,
                iov_len: b.bytes_init(),
            })
            .collect();

        // Get raw buffer info
        let ptr = iovs.as_ptr();
        let len = iovs.len();

        Self::new(
            WritevData {
                fd: fd.clone(),
                bufs,
                iovs,
            },
            WritevTransform {
                _phantom: PhantomData,
            },
            opcode::Writev::new(types::Fd(fd.raw_fd()), ptr, len as _)
                .offset(offset as _)
                .build(),
        )
    }
}
