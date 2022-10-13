use crate::driver::{Op,op::Completable};
use std::io;

/// No operation. Just posts a completion event, nothing else.
///
/// Has a place in benchmarking.
pub struct NoOp {}

impl Op<NoOp> {
    pub fn no_op() -> io::Result<Op<NoOp>> {
        use io_uring::{opcode};

        Op::submit_with(
            NoOp {},
            |_| {
                opcode::Nop::new().build()
            },
        )
    }
}

impl Completable for NoOp {
    type Output = io::Result<()>;

    fn complete(self, _result: io::Result<u32>, _flags: u32) -> Self::Output {
        Ok(())
    }
}