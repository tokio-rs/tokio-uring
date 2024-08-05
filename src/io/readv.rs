use crate::buf::BoundedBufMut;
use crate::{BufResult, OneshotOutputTransform, UnsubmittedOneshot};

use crate::io::SharedFd;
use io_uring::cqueue::Entry;
use libc::iovec;
use std::io;
use std::marker::PhantomData;

/// An unsubmitted readv operation.
pub type UnsubmittedReadv<T> = UnsubmittedOneshot<ReadvData<T>, ReadvTransform<T>>;

#[allow(missing_docs)]
pub struct ReadvData<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    /// Reference to the in-flight buffer.
    pub(crate) bufs: Vec<T>,

    /// Parameter for `io_uring::op::readv`, referring `bufs`.
    #[allow(dead_code)]
    iovs: Vec<iovec>,
}

#[allow(missing_docs)]
pub struct ReadvTransform<T> {
    _phantom: PhantomData<T>,
}

impl<T> OneshotOutputTransform for ReadvTransform<T>
where
    T: BoundedBufMut,
{
    type Output = BufResult<usize, Vec<T>>;
    type StoredData = ReadvData<T>;

    fn transform_oneshot_output(self, data: Self::StoredData, cqe: Entry) -> Self::Output {
        // Recover the buffer
        let mut bufs = data.bufs;

        let res = if cqe.result() >= 0 {
            // If the operation was successful, advance the initialized cursor.
            let mut count = cqe.result() as usize;
            for b in bufs.iter_mut() {
                let sz = std::cmp::min(count, b.bytes_total() - b.bytes_init());
                let pos = b.bytes_init() + sz;
                // Safety: the kernel returns bytes written, and we have ensured that `pos` is
                // valid for current buffer.
                unsafe { b.set_init(pos) };
                count -= sz;
                if count == 0 {
                    break;
                }
            }
            assert_eq!(count, 0);
            Ok(cqe.result() as usize)
        } else {
            Err(io::Error::from_raw_os_error(-cqe.result()))
        };

        (res, bufs)
    }
}

impl<T: BoundedBufMut> UnsubmittedReadv<T> {
    pub(crate) fn readv_at(fd: &SharedFd, mut bufs: Vec<T>, offset: u64) -> Self {
        use io_uring::{opcode, types};

        let iovs: Vec<iovec> = bufs
            .iter_mut()
            .map(|b| iovec {
                // Safety guaranteed by `BoundedBufMut`.
                iov_base: unsafe { b.stable_mut_ptr().add(b.bytes_init()) as *mut libc::c_void },
                iov_len: b.bytes_total() - b.bytes_init(),
            })
            .collect();

        // Get raw buffer info
        let ptr = iovs.as_ptr();
        let len = iovs.len();

        Self::new(
            ReadvData {
                fd: fd.clone(),
                bufs,
                iovs,
            },
            ReadvTransform {
                _phantom: PhantomData,
            },
            opcode::Readv::new(types::Fd(fd.raw_fd()), ptr, len as _)
                .offset(offset as _)
                .build(),
        )
    }
}
