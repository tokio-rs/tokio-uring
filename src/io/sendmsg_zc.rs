//use crate::buf::BoundedBuf;
use crate::io::SharedFd;
use crate::runtime::driver::op::{Completable, CqeResult, MultiCQEFuture, Op, Updateable};
use crate::runtime::CONTEXT;
use std::io;

pub(crate) struct SendMsgZc {
    #[allow(dead_code)]
    fd: SharedFd,

    pub(crate) msghdr: libc::msghdr,

    /// Hold the number of transmitted bytes
    bytes: usize,
}

impl Op<SendMsgZc, MultiCQEFuture> {
    pub(crate) fn sendmsg_zc(fd: &SharedFd, msghdr: &libc::msghdr) -> io::Result<Self> {
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
                    msghdr: msghdr.clone(),
                    bytes: 0,
                },
                |sendmsg_zc| {
                    opcode::SendMsgZc::new(types::Fd(sendmsg_zc.fd.raw_fd()), &sendmsg_zc.msghdr as *const _).build()
                },
            )
        })
    }
}

impl Completable for SendMsgZc {
    type Output = io::Result<usize>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        // Convert the operation result to `usize`
        let res = cqe.result.map(|v| v as usize);
    
        res
    }
}

impl Updateable for SendMsgZc {
    fn update(&mut self, cqe: CqeResult) {
        // uring send_zc promises there will be no error on CQE's marked more
        self.bytes += *cqe.result.as_ref().unwrap() as usize;
    }
}
