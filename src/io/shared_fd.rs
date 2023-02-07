use crate::runtime::driver::op::Op;

use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::prelude::IntoRawFd;
use std::sync::Arc;

struct OwnedFd {
    fd: RawFd,
}

impl OwnedFd {
    pub async fn close(self) -> std::io::Result<()> {
        let res = Op::close(self.fd).unwrap().await;
        if res.is_ok() {
            std::mem::forget(self);
        }
        res
    }
}

impl AsRawFd for OwnedFd {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl FromRawFd for OwnedFd {
    #[inline]
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        OwnedFd { fd }
    }
}

impl IntoRawFd for OwnedFd {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        std::mem::forget(self);
        fd
    }
}

impl Drop for OwnedFd {
    fn drop(&mut self) {
        let _ = unsafe { std::fs::File::from_raw_fd(self.fd) };
    }
}

#[derive(Clone)]
pub(crate) struct SharedFd {
    fd: Arc<OwnedFd>,
}

impl SharedFd {
    // TODO: Replace all instances of SharedFd::new with SharedFd::from_raw_fd
    pub fn new(fd: RawFd) -> Self {
        unsafe { Self::from_raw_fd(fd) }
    }

    // TODO: Replace all instances of SharedFd::raw_fd with SharedFd::as_raw_fd
    pub fn raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    pub async fn close(self) -> std::io::Result<()> {
        match Arc::try_unwrap(self.fd) {
            Ok(fd) => fd.close().await,
            Err(_) => panic!("unexpected in-flight io"),
        }
    }
}

impl AsRawFd for SharedFd {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl FromRawFd for SharedFd {
    #[inline]
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self {
            fd: Arc::new(OwnedFd::from_raw_fd(fd)),
        }
    }
}

impl IntoRawFd for SharedFd {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        match std::sync::Arc::try_unwrap(self.fd) {
            Ok(owned_fd) => owned_fd.into_raw_fd(),
            Err(_) => panic!(""),
        }
    }
}
