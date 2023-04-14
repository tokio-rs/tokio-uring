use crate::buf::BoundedBuf;
use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use crate::{BufError, BufResult};
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
    socket_addr: Option<Box<SockAddr>>,
    pub(crate) msghdr: Box<libc::msghdr>,
}

impl<T: BoundedBuf> Op<SendTo<T>> {
    pub(crate) fn send_to(
        fd: &SharedFd,
        buf: T,
        socket_addr: Option<SocketAddr>,
    ) -> io::Result<Op<SendTo<T>>> {
        use io_uring::{opcode, types};

        let io_slices = vec![IoSlice::new(unsafe {
            std::slice::from_raw_parts(buf.stable_ptr(), buf.bytes_init())
        })];

        let mut msghdr: Box<libc::msghdr> = Box::new(unsafe { std::mem::zeroed() });
        msghdr.msg_iov = io_slices.as_ptr() as *mut _;
        msghdr.msg_iovlen = io_slices.len() as _;

        let socket_addr = match socket_addr {
            Some(_socket_addr) => {
                let socket_addr = Box::new(SockAddr::from(_socket_addr));
                msghdr.msg_name = socket_addr.as_ptr() as *mut libc::c_void;
                msghdr.msg_namelen = socket_addr.len();
                Some(socket_addr)
            }
            None => {
                msghdr.msg_name = std::ptr::null_mut();
                msghdr.msg_namelen = 0;
                None
            }
        };

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
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
        })
    }
}

impl<T> Completable for SendTo<T> {
    type Output = BufResult<usize, T>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffer
        let buf = self.buf;

        match res {
            Ok(n) => Ok((n, buf)),
            Err(e) => Err(BufError(e, buf)),
        }
    }
}
