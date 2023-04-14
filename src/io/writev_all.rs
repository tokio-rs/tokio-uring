use crate::BufError;
use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use crate::{buf::BoundedBuf, io::SharedFd};
use libc::iovec;
use std::io;

// This provides a common write-all implementation for writev and is fairly efficient by allocating
// the Vec<iovec> just once, and computing the individual iovec entries just once, at the cost of
// some unsafe calls to advance the iovec array pointer and the iovec_base pointer from time to
// time when a further call to `writev` is necessary.
//
// The fd, bufs, and iovecs resources are passed to and from the operation's future to ensure they
// stay live while the operation is active, even if the future returned by this call is cancelled.
// The SharedFd is only cloned once but at the cost of also passing it back and forth within this
// module.
pub(crate) async fn writev_at_all<T: BoundedBuf>(
    fd: &SharedFd,
    mut bufs: Vec<T>,
    offset: Option<u64>,
) -> crate::BufResult<usize, Vec<T>> {
    // TODO decide if the function should return immediately if all the buffer lengths
    // were to sum to zero. That would save an allocation and one call into writev.

    // The fd is cloned once.
    let mut fd = fd.clone();

    // iovs is allocated once.
    let mut iovs: Vec<iovec> = bufs
        .iter_mut()
        .map(|b| iovec {
            iov_base: b.stable_ptr() as *mut libc::c_void,
            iov_len: b.bytes_init(),
        })
        .collect();

    let mut iovs_ptr = iovs.as_ptr();
    let mut iovs_len: u32 = iovs.len() as _;

    let mut total: usize = 0;

    // Loop until all the bytes have been written or an error has been returned by the io_uring
    // device.

    loop {
        // If caller provided some offset, pass an updated offset to writev
        // else keep passing zero.
        let o = match offset {
            Some(m) => m + (total as u64),
            None => 0,
        };

        // Call the Op that is internal to this module.
        let op = Op::writev_at_all2(fd, bufs, iovs, iovs_ptr, iovs_len, o).unwrap();
        let res;
        (res, fd, bufs, iovs) = op.await;

        let mut n: usize = match res {
            Ok(m) => m,

            // On error, there is no indication how many bytes were written. This is standard.
            // The device doesn't tell us that either.
            Err(e) => return Err(BufError(e, bufs)),
        };

        // TODO if n is zero, while there was more data to be written, should this be interpreted
        // as the file is closed so an error should be returned? Otherwise we reach the
        // unreachable! panic below.
        //
        // if n == 0 { return Err(..); }

        total += n;

        // Consume n and iovs_len until one or the other is exhausted.
        while n != 0 && iovs_len > 0 {
            // safety: iovs_len > 0, so safe to dereference the const *.
            let mut iovec = unsafe { *iovs_ptr };
            let iov_len = iovec.iov_len;
            if n >= iov_len {
                n -= iov_len;
                // safety: iovs_len > 0, so safe to add 1 as iovs_len is decremented by 1.
                iovs_ptr = unsafe { iovs_ptr.add(1) };
                iovs_len -= 1;
            } else {
                // safety: n was found to be less than iov_len, so adding to base and keeping
                // iov_len updated by decrementing maintains the invariant of the iovec
                // representing how much of the buffer remains to be written to.
                iovec.iov_base = unsafe { (iovec.iov_base as *const u8).add(n) } as _;
                iovec.iov_len -= n;
                n = 0;
            }
        }

        // Assert that both n and iovs_len become exhausted simultaneously.

        if (iovs_len == 0 && n != 0) || (iovs_len > 0 && n == 0) {
            unreachable!();
        }

        // We are done when n and iovs_len have been consumed.
        if n == 0 {
            break;
        }
    }
    Ok((total, bufs))
}

struct WritevAll<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    fd: SharedFd,

    bufs: Vec<T>,

    iovs: Vec<iovec>,
}

impl<T: BoundedBuf> Op<WritevAll<T>> {
    fn writev_at_all2(
        // Three values to share to keep live.
        fd: SharedFd,
        bufs: Vec<T>,
        iovs: Vec<iovec>,

        // Three values to use for this invocation.
        iovs_ptr: *const iovec,
        iovs_len: u32,
        offset: u64,
    ) -> io::Result<Op<WritevAll<T>>> {
        use io_uring::{opcode, types};

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                WritevAll { fd, bufs, iovs },
                // So this wouldn't need to be a function. Just pass in the entry.
                |write| {
                    opcode::Writev::new(types::Fd(write.fd.raw_fd()), iovs_ptr, iovs_len)
                        .offset64(offset as _)
                        .build()
                },
            )
        })
    }
}

impl<T> Completable for WritevAll<T>
where
    T: BoundedBuf,
{
    type Output = (Result<usize, io::Error>, SharedFd, Vec<T>, Vec<iovec>);

    fn complete(self, cqe: CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);

        (res, self.fd, self.bufs, self.iovs)
    }
}
