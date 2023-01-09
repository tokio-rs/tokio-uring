use crate::runtime::driver::op::{Completable, CqeResult, Op};
use crate::runtime::CONTEXT;
use std::io;

/// No operation. Just posts a completion event, nothing else.
///
/// Has a place in benchmarking.
pub struct NoOp {}

impl Op<NoOp> {
    pub fn no_op() -> io::Result<Op<NoOp>> {
        use io_uring::opcode;

        CONTEXT.with(|x| {
            x.handle()
                .expect("Not in a runtime context")
                .submit_op(NoOp {}, |_| opcode::Nop::new().build())
        })
    }
}

impl Completable for NoOp {
    type Output = io::Result<()>;

    fn complete(self, cqe: CqeResult) -> Self::Output {
        cqe.result.map(|_| ())
    }
}

#[cfg(test)]
mod test {
    use crate as tokio_uring;

    #[tokio_uring::test]
    async fn perform_no_op() -> () {
        tokio_uring::no_op().await.unwrap();
    }
}
