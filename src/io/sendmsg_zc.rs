use crate::buf::BoundedBuf;
use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use crate::BufResult;
use socket2::SockAddr;
use std::io::IoSlice;
use std::{boxed::Box, io, net::SocketAddr};

pub(crate) struct SendMsgZc<T> {
    #[allow(dead_code)]
    fd: SharedFd,

    /*pub(crate) buf: T,
    
    #[allow(dead_code)]
    io_slices: Vec<IoSlice<'static>>,
    
    #[allow(dead_code)]
    socket_addr: Box<SockAddr>,*/

    pub(crate) msghdr: Box<libc::msghdr>,
}

impl<T: Box<libc::msghdr>> Op<SendZc<T>, MultiCQEFuture> {
    pub(crate) fn sendmsg_zc(fd: &SharedFd, msghdr: Box<libc::msghdr>) -> io::Result<Self> {
        use io_uring::{opcode, types};

        /*let io_slices = vec![IoSlice::new(unsafe {
            std::slice::from_raw_parts(buf.stable_ptr(), buf.bytes_init())
        })];

        let socket_addr = Box::new(SockAddr::from(socket_addr));

        let mut msghdr: Box<libc::msghdr> = Box::new(unsafe { std::mem::zeroed() });
        msghdr.msg_iov = io_slices.as_ptr() as *mut _;
        msghdr.msg_iovlen = io_slices.len() as _;
        msghdr.msg_name = socket_addr.as_ptr() as *mut libc::c_void;
        msghdr.msg_namelen = socket_addr.len();*/

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                SendMsgZc {
                    fd: fd.clone(),
                    /*buf,
                    io_slices,
                    socket_addr,*/
                    msghdr: msghdr.clone(),
                },
                |sendmsg_zc| {
                    opcode::SendMsgZc::new(types::Fd(sendmsg_zc.fd.raw_fd()), sendmsg_zc.msghdr.as_ref() as *const _).build()
                },
            )
        })
    }
}

impl<T> Completable for SendMsgZc<T> {
    type Output = BufResult<usize, T>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let buf = self.buf;

        (res, buf)
    }
}
