use crate::buf::IoBuf;
use crate::driver::op::{self, Buildable, Completable};
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
        let io_slices = vec![IoSlice::new(unsafe {
            std::slice::from_raw_parts(buf.stable_ptr(), buf.bytes_init())
        })];

        let socket_addr = Box::new(SockAddr::from(socket_addr));

        let mut msghdr: Box<libc::msghdr> = Box::new(unsafe { std::mem::zeroed() });
        msghdr.msg_iov = io_slices.as_ptr() as *mut _;
        msghdr.msg_iovlen = io_slices.len() as _;
        msghdr.msg_name = socket_addr.as_ptr() as *mut libc::c_void;
        msghdr.msg_namelen = socket_addr.len();

        SendTo {
            fd: fd.clone(),
            buf,
            io_slices,
            socket_addr,
            msghdr,
        }
        .submit()
    }
}

impl<T: IoBuf> op::Buildable for SendTo<T>
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};

        opcode::SendMsg::new(
            types::Fd(self.fd.raw_fd()),
            self.msghdr.as_ref() as *const _,
        )
        .build()
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
