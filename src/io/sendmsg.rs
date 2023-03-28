use crate::buf::BoundedBuf;
use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use socket2::SockAddr;
use std::io;
use std::io::IoSlice;
use std::net::SocketAddr;

pub(crate) struct SendMsg<T, U> {
    _fd: SharedFd,
    _io_bufs: Vec<T>,
    _io_slices: Vec<IoSlice<'static>>,
    _socket_addr: Option<Box<SockAddr>>,
    msg_control: Option<U>,
    msghdr: libc::msghdr,
}

impl<T: BoundedBuf, U: BoundedBuf> Op<SendMsg<T, U>> {
    pub(crate) fn sendmsg(
        fd: &SharedFd,
        io_bufs: Vec<T>,
        socket_addr: Option<SocketAddr>,
        msg_control: Option<U>,
    ) -> io::Result<Self> {
        use io_uring::{opcode, types};

        let mut msghdr: libc::msghdr = unsafe { std::mem::zeroed() };

        let mut io_slices: Vec<IoSlice<'static>> = Vec::with_capacity(io_bufs.len());

        for io_buf in &io_bufs {
            io_slices.push(IoSlice::new(unsafe {
                std::slice::from_raw_parts(io_buf.stable_ptr(), io_buf.bytes_init())
            }))
        }

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

        match msg_control {
            Some(ref _msg_control) => {
                msghdr.msg_control = _msg_control.stable_ptr() as *mut _;
                msghdr.msg_controllen = _msg_control.bytes_init();
            }
            None => {
                msghdr.msg_control = std::ptr::null_mut();
                msghdr.msg_controllen = 0_usize;
            }
        }

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                SendMsg {
                    _fd: fd.clone(),
                    _io_bufs: io_bufs,
                    _socket_addr: socket_addr,
                    _io_slices: io_slices,
                    msg_control,
                    msghdr,
                },
                |sendmsg| {
                    opcode::SendMsg::new(
                        types::Fd(sendmsg._fd.raw_fd()),
                        &sendmsg.msghdr as *const _,
                    )
                    .build()
                },
            )
        })
    }
}

impl<T, U> Completable for SendMsg<T, U> {
    type Output = (io::Result<usize>, Vec<T>, Option<U>);

    fn complete(self, cqe: CqeResult) -> (io::Result<usize>, Vec<T>, Option<U>) {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|n| n as usize);

        // Recover the data buffers.
        let io_bufs = self._io_bufs;

        // Recover the ancillary data buffer.
        let msg_control = self.msg_control;

        (res, io_bufs, msg_control)
    }
}
