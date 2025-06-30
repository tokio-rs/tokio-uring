use crate::io::{SharedFd, Socket};
use crate::runtime::driver::op;
use crate::runtime::driver::op::{Completable, Op};
use crate::runtime::CONTEXT;
use std::net::SocketAddr;
use std::{boxed::Box, io};

pub(crate) struct Accept {
    fd: SharedFd,
    pub(crate) socketaddr: Box<(libc::sockaddr_storage, libc::socklen_t)>,
}

impl Op<Accept> {
    pub(crate) fn accept(fd: &SharedFd) -> io::Result<Op<Accept>> {
        use io_uring::{opcode, types};

        let socketaddr = Box::new((
            unsafe { std::mem::zeroed() },
            std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t,
        ));
        CONTEXT.with(|x| {
            x.handle().expect("Not in a runtime context").submit_op(
                Accept {
                    fd: fd.clone(),
                    socketaddr,
                },
                |accept| {
                    opcode::Accept::new(
                        types::Fd(accept.fd.raw_fd()),
                        &mut accept.socketaddr.0 as *mut _ as *mut _,
                        &mut accept.socketaddr.1,
                    )
                    .flags(libc::O_CLOEXEC)
                    .build()
                },
            )
        })
    }
}

impl Completable for Accept {
    type Output = io::Result<(Socket, Option<SocketAddr>)>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        let fd = cqe.result?;
        let fd = SharedFd::new(fd as i32);
        let socket = Socket { fd };
        let (_, addr) = unsafe {
            socket2::SockAddr::try_init(move |addr_storage, len| {
                self.socketaddr.0.clone_into(&mut *addr_storage);
                *len = self.socketaddr.1;
                Ok(())
            })?
        };
        Ok((socket, addr.as_socket()))
    }
}
