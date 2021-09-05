use std::io;
use std::net::SocketAddr;
use std::pin::Pin;

use crate::driver::{Op, SharedFd};
use crate::driver::util::{socket_addr, SocketAddrCRepr};

pub(crate) struct Connect {
    _fd: SharedFd,
    _addr: Pin<Box<SocketAddrCRepr>>,
}

impl Op<Connect> {
    pub(crate) fn connect(fd: SharedFd, address: &SocketAddr) -> io::Result<Self> {
        use io_uring::{opcode, types};

        let (addr, len) = socket_addr(address);

        let boxed = Box::pin(addr);

        let sock_ptr = boxed.as_ptr();

        let connect = Connect {
            _fd: fd.clone(),
            _addr: boxed,
        };
        
        let state_fn = || opcode::Connect::new(types::Fd(fd.raw_fd()), sock_ptr, len).build();

        Op::submit_with(connect, state_fn)
    }
}