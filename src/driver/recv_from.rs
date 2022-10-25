use crate::driver::op::{self, Completable};
use crate::{
    buf::IoBufMut,
    driver::{Op, SharedFd},
    BufResult,
};
use socket2::SockAddr;
use std::{
    io::IoSliceMut,
    {boxed::Box, io, net::SocketAddr},
};

#[allow(dead_code)]
pub(crate) struct RecvFrom<T> {
    fd: SharedFd,
    pub(crate) buf: T,
    io_slices: Vec<IoSliceMut<'static>>,
    pub(crate) socket_addr: Box<SockAddr>,
    pub(crate) msghdr: Box<libc::msghdr>,
}

impl<T: IoBufMut> Op<RecvFrom<T>> {
    pub(crate) fn recv_from(fd: &SharedFd, mut buf: T) -> io::Result<Op<RecvFrom<T>>> {
        use io_uring::{opcode, types};

        let mut io_slices = vec![IoSliceMut::new(unsafe {
            std::slice::from_raw_parts_mut(buf.stable_mut_ptr(), buf.bytes_total())
        })];

        let socket_addr = Box::new(unsafe { SockAddr::init(|_, _| Ok(()))?.1 });

        let mut msghdr: Box<libc::msghdr> = Box::new(unsafe { std::mem::zeroed() });
        msghdr.msg_iov = io_slices.as_mut_ptr().cast();
        msghdr.msg_iovlen = io_slices.len() as _;
        msghdr.msg_name = socket_addr.as_ptr() as *mut libc::c_void;
        msghdr.msg_namelen = socket_addr.len();

        Op::submit_with(
            RecvFrom {
                fd: fd.clone(),
                buf,
                io_slices,
                socket_addr,
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
}

impl<T> Completable for RecvFrom<T>
where
    T: IoBufMut,
{
    type Output = BufResult<(usize, SocketAddr), T>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let mut buf = self.buf;

        let socket_addr = (*self.socket_addr).as_socket();

        let res = res.map(|n| {
            let socket_addr: SocketAddr = socket_addr.unwrap();

            // Safety: the kernel wrote `n` bytes to the buffer.
            unsafe {
                buf.set_init(n);
            }

            (n, socket_addr)
        });

        (res, buf)
    }
}
