use crate::buf::BoundedBuf;
use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, MultiCQEFuture, Op, Updateable};
use crate::runtime::CONTEXT;
use socket2::SockAddr;
use std::io;
use std::io::IoSlice;
use std::net::SocketAddr;

pub(crate) struct SendMsgZc<T, U> {
    #[allow(dead_code)]
    fd: SharedFd,
    #[allow(dead_code)]
    io_bufs: Vec<T>,
    #[allow(dead_code)]
    io_slices: Vec<IoSlice<'static>>,
    socket_addr: Option<Box<SockAddr>>,
    msg_control: Option<U>,
    msghdr: libc::msghdr,

    /// Hold the number of transmitted bytes
    bytes: usize,
}

impl<T: BoundedBuf, U: BoundedBuf> Op<SendMsgZc<T, U>, MultiCQEFuture> {
    pub(crate) fn sendmsg_zc(
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
                SendMsgZc {
                    fd: fd.clone(),
                    io_bufs,
                    socket_addr,
                    io_slices,
                    msg_control,
                    msghdr,
                    bytes: 0,
                },
                |sendmsg_zc| {
                    opcode::SendMsgZc::new(
                        types::Fd(sendmsg_zc.fd.raw_fd()),
                        &sendmsg_zc.msghdr as *const _,
                    )
                    .build()
                },
            )
        })
    }
}

impl<T, U> Completable for SendMsgZc<T, U> {
    type Output = (io::Result<usize>, Vec<T>, Option<U>);

    fn complete(self, cqe: CqeResult) -> (io::Result<usize>, Vec<T>, Option<U>) {
        // Convert the operation result to `usize`, and add previous byte count
        let res = cqe.result.map(|v| self.bytes + v as usize);

        // Recover the data buffers.
        let io_bufs = self.io_bufs;

        // Recover the ancillary data buffer.
        let msg_control = self.msg_control;

        (res, io_bufs, msg_control)
    }
}

impl<T, U> Updateable for SendMsgZc<T, U> {
    fn update(&mut self, cqe: CqeResult) {
        // uring send_zc promises there will be no error on CQE's marked more
        self.bytes += *cqe.result.as_ref().unwrap() as usize;
    }
}
