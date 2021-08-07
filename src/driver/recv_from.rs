use crate::{
    buf::IoBufMut,
    driver::{Op, SharedFd},
    BufResult,
};
use os_socketaddr::OsSocketAddr;
use std::{
    io::IoSliceMut,
    task::{Context, Poll},
    {boxed::Box, io, net::SocketAddr},
};

#[allow(dead_code)]
pub(crate) struct RecvFrom<T> {
    fd: SharedFd,
    pub(crate) buf: T,
    io_slices: Vec<IoSliceMut<'static>>,
    pub(crate) os_socket_addr: Box<OsSocketAddr>,
    pub(crate) msghdr: Box<libc::msghdr>,
}

impl<T: IoBufMut> Op<RecvFrom<T>> {
    pub(crate) fn recv_from(fd: &SharedFd, mut buf: T) -> io::Result<Op<RecvFrom<T>>> {
        use io_uring::{opcode, types};

        let mut io_slices = vec![IoSliceMut::new(unsafe {
            std::slice::from_raw_parts_mut(buf.stable_mut_ptr(), buf.bytes_total())
        })];

        let mut os_socket_addr = Box::new(OsSocketAddr::new());

        let mut msghdr: Box<libc::msghdr> = Box::new(unsafe { std::mem::zeroed() });
        msghdr.msg_iov = io_slices.as_mut_ptr().cast();
        msghdr.msg_iovlen = io_slices.len() as _;
        msghdr.msg_name = os_socket_addr.as_mut_ptr() as *mut libc::c_void;
        msghdr.msg_namelen = os_socket_addr.capacity();

        Op::submit_with(
            RecvFrom {
                fd: fd.clone(),
                buf,
                io_slices,
                os_socket_addr,
                msghdr,
            },
            |recv_from| {
                opcode::RecvMsg::new(
                    types::Fd(recv_from.fd.raw_fd()),
                    recv_from.msghdr.as_mut() as *mut _,
                )
                .build()
            },
        )
    }

    pub(crate) async fn recv(mut self) -> BufResult<(usize, SocketAddr), T> {
        use crate::future::poll_fn;

        poll_fn(move |cx| self.poll_recv_from(cx)).await
    }

    pub(crate) fn poll_recv_from(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<BufResult<(usize, SocketAddr), T>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));

        // Recover the buffer
        let mut buf = complete.data.buf;

        let result = match complete.result {
            Ok(v) => {
                let v = v as usize;
                let socket_addr: Option<SocketAddr> = (*complete.data.os_socket_addr).into();
                // If the operation was successful, advance the initialized cursor.
                // Safety: the kernel wrote `v` bytes to the buffer.
                unsafe {
                    buf.set_init(v);
                }
                Ok((v, socket_addr.unwrap()))
            }
            Err(e) => Err(e),
        };
        Poll::Ready((result, buf))
    }
}
