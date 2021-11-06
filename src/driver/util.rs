use std::ffi::CString;
use std::io;
use std::mem::size_of;
use std::net::SocketAddr;
use std::path::Path;

pub(super) fn cstr(p: &Path) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    Ok(CString::new(p.as_os_str().as_bytes())?)
}

/// A type with the same memory layout as `libc::sockaddr`. Used in converting Rust level
/// SocketAddr* types into their system representation. The benefit of this specific
/// type over using `libc::sockaddr_storage` is that this type is exactly as large as it
/// needs to be and not a lot larger. And it can be initialized cleaner from Rust.
#[repr(C)]
pub(crate) union SocketAddrCRepr {
    v4: libc::sockaddr_in,
    v6: libc::sockaddr_in6,
}

impl SocketAddrCRepr {
    pub(crate) fn as_ptr(&self) -> *const libc::sockaddr {
        self as *const _ as *const libc::sockaddr
    }
}

/// Converts a Rust `SocketAddr` into the system representation.
pub(crate) fn socket_addr(addr: &SocketAddr) -> (SocketAddrCRepr, libc::socklen_t) {
    match addr {
        SocketAddr::V4(ref addr) => {
            // `s_addr` is stored as BE on all machine and the array is in BE order.
            // So the native endian conversion method is used so that it's never swapped.
            let sin_addr = libc::in_addr {
                s_addr: u32::from_ne_bytes(addr.ip().octets()),
            };

            let sockaddr_in = libc::sockaddr_in {
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: addr.port().to_be(),
                sin_addr,
                sin_zero: [0; 8],
                #[cfg(any(
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "ios",
                    target_os = "macos",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                sin_len: 0,
            };

            let sockaddr = SocketAddrCRepr { v4: sockaddr_in };
            let socklen = size_of::<libc::sockaddr_in>() as libc::socklen_t;
            (sockaddr, socklen)
        }
        SocketAddr::V6(ref addr) => {
            let sockaddr_in6 = libc::sockaddr_in6 {
                sin6_family: libc::AF_INET6 as libc::sa_family_t,
                sin6_port: addr.port().to_be(),
                sin6_addr: libc::in6_addr {
                    s6_addr: addr.ip().octets(),
                },
                sin6_flowinfo: addr.flowinfo(),
                sin6_scope_id: addr.scope_id(),
                #[cfg(any(
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "ios",
                    target_os = "macos",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                sin6_len: 0,
                #[cfg(any(target_os = "solaris", target_os = "illumos"))]
                __sin6_src_id: 0,
            };

            let sockaddr = SocketAddrCRepr { v6: sockaddr_in6 };
            let socklen = size_of::<libc::sockaddr_in6>() as libc::socklen_t;
            (sockaddr, socklen)
        }
    }
}
