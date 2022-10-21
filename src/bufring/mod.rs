//! Bindings for the io_uring interface's buf_ring feature.
//!

//mod br_buf;

pub(crate) mod ring;

pub use ring::BufRing;
pub use ring::BufRingRc;
pub use ring::Builder;

// TODO Maybe create a bufringid module
// with a trivial implementation of a type that tracks the next bufringid to use.
