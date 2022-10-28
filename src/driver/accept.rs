use crate::driver::op::{self, Completable};
use crate::driver::{Op, SharedFd, Socket};
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
        Op::submit_with(
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
    }
}

impl Completable for Accept {
    type Output = io::Result<(Socket, Option<SocketAddr>)>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        let fd = cqe.result?;
        let fd = SharedFd::new(fd as i32);
        let socket = Socket { fd };
        let (_, addr) = unsafe {
            socket2::SockAddr::init(move |addr_storage, len| {
                *addr_storage = self.socketaddr.0.to_owned();
                *len = self.socketaddr.1;
                Ok(())
            })?
        };
        Ok((socket, addr.as_socket()))
    }
}
