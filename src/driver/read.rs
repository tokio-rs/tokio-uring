use crate::BufMutResult;
use crate::buf::{self, IoBufMut};
use crate::driver::Op;

use futures::ready;
use std::io;
use std::os::unix::io::RawFd;
use std::task::{Context, Poll};

pub(crate) struct Read {
    /// Reference to the in-flight buffer.
    pub(crate) buf: Option<buf::SliceMut>,
}

impl Op<Read> {
    pub(crate) fn read_at(fd: RawFd, mut buf: buf::SliceMut, offset: u64) -> io::Result<Op<Read>> {
        use io_uring::{opcode, types};

        // Get raw buffer info
        let ptr = buf.as_mut_ptr();
        let len = buf.capacity();

        Op::submit_with(Read {
            buf: Some(buf),
        }, |_| {
            opcode::Read::new(types::Fd(fd), ptr, len as _)
                .offset(offset as _)
                .build()
        })
    }

    pub(crate) fn read_at2(fd: RawFd, offset: u64, len: usize) -> io::Result<Op<Read>> {
        use io_uring::{opcode, types, squeue::Flags};
        use std::ptr;

        Op::submit_with(Read {
            buf: None,
        }, |_| {
            opcode::Read::new(types::Fd(fd), ptr::null_mut(), len as _)
                .offset(offset as _)
                .buf_group(0)
                .build()
                .flags(Flags::BUFFER_SELECT)
        })
    }

    pub(crate) async fn read(mut self) -> BufMutResult<usize> {
        futures::future::poll_fn(move |cx| self.poll_read(cx)).await
    }

    pub(crate) async fn read2(mut self) -> io::Result<IoBufMut> {
        futures::future::poll_fn(|cx| self.poll_read2(cx)).await
    }

    pub(crate) fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<BufMutResult<usize>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));

        // Convert the operation result to `usize`
        let res = complete.result.map(|v| v as usize);
        // Recover the buffer
        let mut buf = complete.state.buf.unwrap();

        // If the operation was successful, advance the initialized cursor.
        if let Ok(n) = res {
            let new_len = buf.begin() + n;
            unsafe { buf.get_inner_mut().set_len(new_len); }
        }

        Poll::Ready((res, buf))
    }

    pub(crate) fn poll_read2(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<IoBufMut>> {
        use io_uring::cqueue::buffer_select;
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(&mut *self).poll(cx));
        let n = complete.result?;
        let bid = buffer_select(complete.flags)
            .expect("unimplemented: why not?");

        println!("bid = {:?}; n = {:?}", bid, n);

        let driver = self.driver.borrow();

        let buf = unsafe {
            let mut buf = driver.pool.checkout(bid, &self.driver);
            buf.vec_mut().set_len(n as _);
            buf
        };

        Poll::Ready(Ok(IoBufMut::from_provided(buf)))
    }
}