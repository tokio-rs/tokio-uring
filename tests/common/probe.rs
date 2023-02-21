use io_uring::{opcode, IoUring, Probe};

pub fn register_probe() -> Option<Probe> {
    let io_uring = match IoUring::new(2) {
        Ok(io_uring) => io_uring,
        Err(_) => {
            return None;
        }
    };
    let submitter = io_uring.submitter();

    let mut probe = Probe::new();

    // register_probe has been available since 5.6.
    if submitter.register_probe(&mut probe).is_err() {
        return None;
    }
    return Some(probe);
}

pub fn is_kernel_minimum_5_19() -> bool {
    let Some(register_probe) = register_probe()
        else {
            return false;
        };

    // IORING_OP_SOCKET was introduced in 5.19.
    register_probe.is_supported(opcode::Socket::CODE)
}

pub fn is_buf_ring_supported() -> bool {
    is_kernel_minimum_5_19()
}
