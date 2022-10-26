use crate::buf::IoBuf;
use crate::driver::op::{self, Completable};
use crate::driver::{Op, SharedFd};
use crate::BufResult;
use socket2::SockAddr;
use std::io::IoSlice;
use std::{boxed::Box, io, net::SocketAddr};

pub(crate) struct SendTo<T> {
    #[allow(dead_code)]
    fd: SharedFd,
    pub(crate) buf: T,
    #[allow(dead_code)]
    io_slices: Vec<IoSlice<'static>>,
    #[allow(dead_code)]
    socket_addr: Box<SockAddr>,
    pub(crate) msghdr: Box<libc::msghdr>,
}

impl<T: IoBuf> Op<SendTo<T>> {
    pub(crate) fn send_to(
        fd: &SharedFd,
        buf: T,
        socket_addr: SocketAddr,
    ) -> io::Result<Op<SendTo<T>>> {
        use io_uring::{opcode, types};

        let io_slices = vec![IoSlice::new(unsafe {
            std::slice::from_raw_parts(buf.stable_ptr(), buf.bytes_init())
        })];

        let socket_addr = Box::new(SockAddr::from(socket_addr));

        let mut msghdr: Box<libc::msghdr> = Box::new(unsafe { std::mem::zeroed() });
        msghdr.msg_iov = io_slices.as_ptr() as *mut _;
        msghdr.msg_iovlen = io_slices.len() as _;
        msghdr.msg_name = socket_addr.as_ptr() as *mut libc::c_void;
        msghdr.msg_namelen = socket_addr.len();

        Op::submit_with(
            SendTo {
                fd: fd.clone(),
                buf,
                io_slices,
                socket_addr,
                msghdr,
            },
            |send_to| {
                opcode::SendMsg::new(
                    types::Fd(send_to.fd.raw_fd()),
                    send_to.msghdr.as_ref() as *const _,
                )
                .build()
            },
        )
    }
}

impl<T> Completable for SendTo<T>
where
    T: IoBuf,
{
    type Output = BufResult<usize, T>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let buf = self.buf;

        (res, buf)
    }
}
