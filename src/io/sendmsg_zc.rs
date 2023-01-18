use crate::buf::IoBuf;
use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, MultiCQEFuture, Op, Updateable};
use crate::runtime::CONTEXT;
use socket2::SockAddr;
use std::io;
use std::net::SocketAddr;

pub(crate) struct SendMsgZc<T> {
    #[allow(dead_code)]
    fd: SharedFd,
    #[allow(dead_code)]
    io_bufs: Vec<T>,
    #[allow(dead_code)]
    socket_addr: Box<SockAddr>,
    msg_control: Option<T>,
    msghdr: libc::msghdr,

    /// Hold the number of transmitted bytes
    bytes: usize,
}

impl<T: IoBuf> Op<SendMsgZc<T>, MultiCQEFuture> {
    pub(crate) fn sendmsg_zc(
        fd: &SharedFd,
        io_bufs: Vec<T>,
        socket_addr: SocketAddr,
        msg_control: Option<T>,
    ) -> io::Result<Self> {
        use io_uring::{opcode, types};

        let socket_addr = Box::new(SockAddr::from(socket_addr));

        let mut msghdr: libc::msghdr = unsafe { std::mem::zeroed() };

        msghdr.msg_iov = io_bufs.as_ptr() as *mut _;
        msghdr.msg_iovlen = io_bufs.len() as _;
        msghdr.msg_name = socket_addr.as_ptr() as *mut libc::c_void;
        msghdr.msg_namelen = socket_addr.len();

        match msg_control {
            Some(ref _msg_control) => {
                msghdr.msg_control = _msg_control.stable_ptr() as *mut _;
                msghdr.msg_controllen = _msg_control.bytes_init();
            }
            None => {
                msghdr.msg_control = std::ptr::null_mut();
                msghdr.msg_controllen = 0 as usize;
            }
        }

        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                SendMsgZc {
                    fd: fd.clone(),
                    io_bufs,
                    socket_addr,
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

impl<T> Completable for SendMsgZc<T> {
    type Output = (io::Result<usize>, Vec<T>, Option<T>);

    fn complete(self, cqe: CqeResult) -> (io::Result<usize>, Vec<T>, Option<T>) {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);

        // Recover the data buffers.
        let io_bufs = self.io_bufs;

        // Recover the ancillary data buffer.
        let msg_control = self.msg_control;

        (res, io_bufs, msg_control)
    }
}

impl<T> Updateable for SendMsgZc<T> {
    fn update(&mut self, cqe: CqeResult) {
        // uring send_zc promises there will be no error on CQE's marked more
        self.bytes += *cqe.result.as_ref().unwrap() as usize;
    }
}
