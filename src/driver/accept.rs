use crate::driver::op::{self, Buildable, Completable};
use crate::driver::{Op, SharedFd, Socket};
use std::net::SocketAddr;
use std::{boxed::Box, io};

pub(crate) struct Accept {
    fd: SharedFd,
    pub(crate) socketaddr: Box<(libc::sockaddr_storage, libc::socklen_t)>,
}

impl Op<Accept> {
    pub(crate) fn accept(fd: &SharedFd) -> io::Result<Op<Accept>> {
        let socketaddr = Box::new((
            unsafe { std::mem::zeroed() },
            std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t,
        ));
        Accept {
            fd: fd.clone(),
            socketaddr,
        }
        .submit()
    }
}

impl Buildable for Accept
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        use io_uring::{opcode, types};

        opcode::Accept::new(
            types::Fd(self.fd.raw_fd()),
            &mut self.socketaddr.0 as *mut _ as *mut _,
            &mut self.socketaddr.1,
        )
        .flags(libc::O_CLOEXEC)
        .build()
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
