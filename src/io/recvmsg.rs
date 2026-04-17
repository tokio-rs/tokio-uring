use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use crate::{buf::BoundedBufMut, io::SharedFd, BufResult};
use socket2::SockAddr;
use std::{
    io::IoSliceMut,
    {boxed::Box, io, net::SocketAddr},
};

pub(crate) struct RecvMsg<T, U = Vec<u8>> {
    #[allow(dead_code)]
    fd: SharedFd,
    pub(crate) buf: Vec<T>,
    #[allow(dead_code)]
    io_slices: Vec<IoSliceMut<'static>>,
    pub(crate) socket_addr: Box<SockAddr>,
    pub(crate) msg_control: Option<U>,
    pub(crate) msghdr: Box<libc::msghdr>,
}

impl<T: BoundedBufMut, U: BoundedBufMut> Op<RecvMsg<T, U>> {
    pub(crate) fn recvmsg(
        fd: &SharedFd,
        mut bufs: Vec<T>,
        mut msg_control: Option<U>,
    ) -> io::Result<Op<RecvMsg<T, U>>> {
        use io_uring::{opcode, types};

        let mut io_slices = Vec::with_capacity(bufs.len());
        for buf in &mut bufs {
            io_slices.push(IoSliceMut::new(unsafe {
                std::slice::from_raw_parts_mut(buf.stable_mut_ptr(), buf.bytes_total())
            }));
        }

        let socket_addr = Box::new(unsafe { SockAddr::init(|_, _| Ok(()))?.1 });

        let mut msghdr: Box<libc::msghdr> = Box::new(unsafe { std::mem::zeroed() });
        msghdr.msg_iov = io_slices.as_mut_ptr().cast();
        msghdr.msg_iovlen = io_slices.len() as _;
        msghdr.msg_name = socket_addr.as_ptr() as *mut libc::c_void;
        msghdr.msg_namelen = socket_addr.len();
        if let Some(msg_control) = &mut msg_control {
            msghdr.msg_control = msg_control.stable_mut_ptr().cast();
            msghdr.msg_controllen = msg_control.bytes_total();
        }

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                RecvMsg {
                    fd: fd.clone(),
                    buf: bufs,
                    io_slices,
                    socket_addr,
                    msg_control,
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
        })
    }
}

impl<T, U> Completable for RecvMsg<T, U>
where
    T: BoundedBufMut,
    U: BoundedBufMut,
{
    type Output = BufResult<(usize, SocketAddr, Option<U>), Vec<T>>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
        // Recover the buffers
        let mut bufs = self.buf;

        let socket_addr = (*self.socket_addr).as_socket();

        let msg_control = self.msg_control;

        let res = res.map(|n| {
            let socket_addr: SocketAddr = socket_addr.unwrap();

            let mut bytes = n;
            for buf in &mut bufs {
                // Safety: the kernel wrote `n` bytes to the buffer.
                unsafe {
                    buf.set_init(bytes);
                }
                let total = buf.bytes_total();
                if bytes > total {
                    bytes -= total;
                } else {
                    // In the current API bytes_init is a watermark,
                    // so remaining don't need zeroing.
                    break;
                }
            }
            (n, socket_addr, msg_control)
        });

        (res, bufs)
    }
}
