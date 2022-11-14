use io_uring::{opcode, IoUring, Probe};
use once_cell::sync::Lazy;

const IORING_OP_NOP_SUPPORTED: u64 = 1 << opcode::Nop::CODE;
const IORING_OP_READV_SUPPORTED: u64 = 1 << opcode::Readv::CODE;
const IORING_OP_WRITEV_SUPPORTED: u64 = 1 << opcode::Writev::CODE;
const IORING_OP_FSYNC_SUPPORTED: u64 = 1 << opcode::Fsync::CODE;
const IORING_OP_READ_FIXED_SUPPORTED: u64 = 1 << opcode::ReadFixed::CODE;
const IORING_OP_WRITE_FIXED_SUPPORTED: u64 = 1 << opcode::WriteFixed::CODE;
const IORING_OP_POLL_ADD_SUPPORTED: u64 = 1 << opcode::PollAdd::CODE;
const IORING_OP_POLL_REMOVE_SUPPORTED: u64 = 1 << opcode::PollRemove::CODE;
const IORING_OP_SYNC_FILE_RANGE_SUPPORTED: u64 = 1 << opcode::SyncFileRange::CODE;
const IORING_OP_SENDMSG_SUPPORTED: u64 = 1 << opcode::SendMsg::CODE;
const IORING_OP_RECVMSG_SUPPORTED: u64 = 1 << opcode::RecvMsg::CODE;
const IORING_OP_TIMEOUT_SUPPORTED: u64 = 1 << opcode::Timeout::CODE;
const IORING_OP_TIMEOUT_REMOVE_SUPPORTED: u64 = 1 << opcode::TimeoutRemove::CODE;
const IORING_OP_ACCEPT_SUPPORTED: u64 = 1 << opcode::Accept::CODE;
const IORING_OP_ASYNC_CANCEL_SUPPORTED: u64 = 1 << opcode::AsyncCancel::CODE;
const IORING_OP_LINK_TIMEOUT_SUPPORTED: u64 = 1 << opcode::LinkTimeout::CODE;
const IORING_OP_CONNECT_SUPPORTED: u64 = 1 << opcode::Connect::CODE;
const IORING_OP_FALLOCATE_SUPPORTED: u64 = 1 << opcode::Fallocate64::CODE;
const IORING_OP_OPENAT_SUPPORTED: u64 = 1 << opcode::OpenAt::CODE;
const IORING_OP_CLOSE_SUPPORTED: u64 = 1 << opcode::Close::CODE;
const IORING_OP_FILES_UPDATE_SUPPORTED: u64 = 1 << opcode::FilesUpdate::CODE;
const IORING_OP_STATX_SUPPORTED: u64 = 1 << opcode::Statx::CODE;
const IORING_OP_READ_SUPPORTED: u64 = 1 << opcode::Read::CODE;
const IORING_OP_WRITE_SUPPORTED: u64 = 1 << opcode::Write::CODE;
const IORING_OP_FADVISE_SUPPORTED: u64 = 1 << opcode::Fadvise::CODE;
const IORING_OP_MADVISE_SUPPORTED: u64 = 1 << opcode::Madvise::CODE;
const IORING_OP_SEND_SUPPORTED: u64 = 1 << opcode::Send::CODE;
const IORING_OP_RECV_SUPPORTED: u64 = 1 << opcode::Recv::CODE;
const IORING_OP_OPENAT2_SUPPORTED: u64 = 1 << opcode::OpenAt2::CODE;
const IORING_OP_EPOLL_CTL_SUPPORTED: u64 = 1 << opcode::EpollCtl::CODE;
const IORING_OP_SPLICE_SUPPORTED: u64 = 1 << opcode::Splice::CODE;
const IORING_OP_PROVIDE_BUFFERS_SUPPORTED: u64 = 1 << opcode::ProvideBuffers::CODE;
const IORING_OP_REMOVE_BUFFERS_SUPPORTED: u64 = 1 << opcode::RemoveBuffers::CODE;
const IORING_OP_TEE_SUPPORTED: u64 = 1 << opcode::Tee::CODE;
const IORING_OP_SHUTDOWN_SUPPORTED: u64 = 1 << opcode::Shutdown::CODE;
const IORING_OP_RENAMEAT_SUPPORTED: u64 = 1 << opcode::RenameAt::CODE;
const IORING_OP_UNLINKAT_SUPPORTED: u64 = 1 << opcode::UnlinkAt::CODE;
const IORING_OP_MKDIRAT_SUPPORTED: u64 = 1 << opcode::MkDirAt::CODE;
const IORING_OP_SYMLINKAT_SUPPORTED: u64 = 1 << opcode::SymlinkAt::CODE;
const IORING_OP_LINKAT_SUPPORTED: u64 = 1 << opcode::LinkAt::CODE;
// The same as opcode::UringCmd80::CODE
const IORING_OP_URING_CMD_SUPPORTED: u64 = 1 << opcode::UringCmd16::CODE;
const IORING_OP_SEND_ZC_SUPPORTED: u64 = 1 << opcode::SendZc::CODE;

pub(crate) static PROBE_INFO: Lazy<ProbeInfo> = Lazy::new(ProbeInfo::new);

/// Information about what `io_uring` features the kernel supports.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ProbeInfo(u64);

impl ProbeInfo {
    pub(crate) fn new() -> Self {
        let mut supported = Self(0);
        let io_uring = match IoUring::new(1) {
            Ok(io_uring) => io_uring,
            Err(_) => {
                return supported;
            }
        };
        let submitter = io_uring.submitter();

        let mut probe = Probe::new();

        if submitter.register_probe(&mut probe).is_err() {
            return supported;
        }
        if probe.is_supported(opcode::Nop::CODE) {
            supported = Self(supported.0 | IORING_OP_NOP_SUPPORTED);
        }
        if probe.is_supported(opcode::Readv::CODE) {
            supported = Self(supported.0 | IORING_OP_READV_SUPPORTED);
        }
        if probe.is_supported(opcode::Writev::CODE) {
            supported = Self(supported.0 | IORING_OP_WRITEV_SUPPORTED);
        }
        if probe.is_supported(opcode::Fsync::CODE) {
            supported = Self(supported.0 | IORING_OP_FSYNC_SUPPORTED);
        }
        if probe.is_supported(opcode::ReadFixed::CODE) {
            supported = Self(supported.0 | IORING_OP_READ_FIXED_SUPPORTED);
        }
        if probe.is_supported(opcode::WriteFixed::CODE) {
            supported = Self(supported.0 | IORING_OP_WRITE_FIXED_SUPPORTED);
        }
        if probe.is_supported(opcode::PollAdd::CODE) {
            supported = Self(supported.0 | IORING_OP_POLL_ADD_SUPPORTED);
        }
        if probe.is_supported(opcode::PollRemove::CODE) {
            supported = Self(supported.0 | IORING_OP_POLL_REMOVE_SUPPORTED);
        }
        if probe.is_supported(opcode::SyncFileRange::CODE) {
            supported = Self(supported.0 | IORING_OP_SYNC_FILE_RANGE_SUPPORTED);
        }
        if probe.is_supported(opcode::SendMsg::CODE) {
            supported = Self(supported.0 | IORING_OP_SENDMSG_SUPPORTED);
        }
        if probe.is_supported(opcode::RecvMsg::CODE) {
            supported = Self(supported.0 | IORING_OP_RECVMSG_SUPPORTED);
        }
        if probe.is_supported(opcode::Timeout::CODE) {
            supported = Self(supported.0 | IORING_OP_TIMEOUT_SUPPORTED);
        }
        if probe.is_supported(opcode::TimeoutRemove::CODE) {
            supported = Self(supported.0 | IORING_OP_TIMEOUT_REMOVE_SUPPORTED);
        }
        if probe.is_supported(opcode::Accept::CODE) {
            supported = Self(supported.0 | IORING_OP_ACCEPT_SUPPORTED);
        }
        if probe.is_supported(opcode::AsyncCancel::CODE) {
            supported = Self(supported.0 | IORING_OP_ASYNC_CANCEL_SUPPORTED);
        }
        if probe.is_supported(opcode::LinkTimeout::CODE) {
            supported = Self(supported.0 | IORING_OP_LINK_TIMEOUT_SUPPORTED);
        }
        if probe.is_supported(opcode::Connect::CODE) {
            supported = Self(supported.0 | IORING_OP_CONNECT_SUPPORTED);
        }
        if probe.is_supported(opcode::Fallocate64::CODE) {
            supported = Self(supported.0 | IORING_OP_FALLOCATE_SUPPORTED);
        }
        if probe.is_supported(opcode::OpenAt::CODE) {
            supported = Self(supported.0 | IORING_OP_OPENAT_SUPPORTED);
        }
        if probe.is_supported(opcode::Close::CODE) {
            supported = Self(supported.0 | IORING_OP_CLOSE_SUPPORTED);
        }
        if probe.is_supported(opcode::FilesUpdate::CODE) {
            supported = Self(supported.0 | IORING_OP_FILES_UPDATE_SUPPORTED);
        }
        if probe.is_supported(opcode::Statx::CODE) {
            supported = Self(supported.0 | IORING_OP_STATX_SUPPORTED);
        }
        if probe.is_supported(opcode::Read::CODE) {
            supported = Self(supported.0 | IORING_OP_READ_SUPPORTED);
        }
        if probe.is_supported(opcode::Write::CODE) {
            supported = Self(supported.0 | IORING_OP_WRITE_SUPPORTED);
        }
        if probe.is_supported(opcode::Fadvise::CODE) {
            supported = Self(supported.0 | IORING_OP_FADVISE_SUPPORTED);
        }
        if probe.is_supported(opcode::Madvise::CODE) {
            supported = Self(supported.0 | IORING_OP_MADVISE_SUPPORTED);
        }
        if probe.is_supported(opcode::Send::CODE) {
            supported = Self(supported.0 | IORING_OP_SEND_SUPPORTED);
        }
        if probe.is_supported(opcode::Recv::CODE) {
            supported = Self(supported.0 | IORING_OP_RECV_SUPPORTED);
        }
        if probe.is_supported(opcode::OpenAt2::CODE) {
            supported = Self(supported.0 | IORING_OP_OPENAT2_SUPPORTED);
        }
        if probe.is_supported(opcode::EpollCtl::CODE) {
            supported = Self(supported.0 | IORING_OP_EPOLL_CTL_SUPPORTED);
        }
        if probe.is_supported(opcode::Splice::CODE) {
            supported = Self(supported.0 | IORING_OP_SPLICE_SUPPORTED);
        }
        if probe.is_supported(opcode::ProvideBuffers::CODE) {
            supported = Self(supported.0 | IORING_OP_PROVIDE_BUFFERS_SUPPORTED);
        }
        if probe.is_supported(opcode::RemoveBuffers::CODE) {
            supported = Self(supported.0 | IORING_OP_REMOVE_BUFFERS_SUPPORTED);
        }
        if probe.is_supported(opcode::Tee::CODE) {
            supported = Self(supported.0 | IORING_OP_TEE_SUPPORTED);
        }
        if probe.is_supported(opcode::Shutdown::CODE) {
            supported = Self(supported.0 | IORING_OP_SHUTDOWN_SUPPORTED);
        }
        if probe.is_supported(opcode::RenameAt::CODE) {
            supported = Self(supported.0 | IORING_OP_RENAMEAT_SUPPORTED);
        }
        if probe.is_supported(opcode::UnlinkAt::CODE) {
            supported = Self(supported.0 | IORING_OP_UNLINKAT_SUPPORTED);
        }
        if probe.is_supported(opcode::MkDirAt::CODE) {
            supported = Self(supported.0 | IORING_OP_MKDIRAT_SUPPORTED);
        }
        if probe.is_supported(opcode::SymlinkAt::CODE) {
            supported = Self(supported.0 | IORING_OP_SYMLINKAT_SUPPORTED);
        }
        if probe.is_supported(opcode::LinkAt::CODE) {
            supported = Self(supported.0 | IORING_OP_LINKAT_SUPPORTED);
        }
        if probe.is_supported(opcode::UringCmd16::CODE) {
            supported = Self(supported.0 | IORING_OP_URING_CMD_SUPPORTED);
        }
        if probe.is_supported(opcode::SendZc::CODE) {
            supported = Self(supported.0 | IORING_OP_SEND_ZC_SUPPORTED);
        }

        supported
    }

    /// Get whether the io-uring is supported.
    pub fn supported(&self) -> bool {
        self.nop_supported()
    }

    /// Get whether the nop is supported.
    pub fn nop_supported(&self) -> bool {
        self.0 & IORING_OP_NOP_SUPPORTED != 0
    }

    /// Get whether the readv is supported.
    pub fn readv_supported(&self) -> bool {
        self.0 & IORING_OP_READV_SUPPORTED != 0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_probe() {
        let supported = ProbeInfo::new();
        assert_eq!(supported, crate::io_uring_probe());
    }
}
