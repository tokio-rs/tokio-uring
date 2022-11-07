use crate::driver::{
    op::{self, Buildable, Completable},
    Op,
};
use std::io;

/// No operation. Just posts a completion event, nothing else.
///
/// Has a place in benchmarking.
pub struct NoOp {}

impl Op<NoOp> {
    pub fn no_op() -> io::Result<Op<NoOp>> {
        NoOp {}.submit()
    }
}

impl Buildable for NoOp
where
    Self: 'static + Sized,
{
    type CqeType = op::SingleCQE;

    fn create_sqe(&mut self) -> io_uring::squeue::Entry {
        io_uring::opcode::Nop::new().build()
    }
}

impl Completable for NoOp {
    type Output = io::Result<()>;

    fn complete(self, cqe: op::CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}

#[cfg(test)]
mod test {
    use crate as tokio_uring;

    #[test]
    fn perform_no_op() -> () {
        tokio_uring::start(async {
            tokio_uring::no_op().await.unwrap();
        })
    }
}
