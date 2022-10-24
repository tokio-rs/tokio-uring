use futures::stream::{self, StreamExt};
use iai::black_box;

#[derive(Clone)]
struct Options {
    iterations: usize,
    concurrency: usize,
    sq_size: usize,
    cq_size: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            iterations: 100000,
            concurrency: 1,
            sq_size: 64,
            cq_size: 256,
        }
    }
}

fn runtime_only() -> Result<(), Box<dyn std::error::Error>> {
    let opts = Options::default();
    let mut ring_opts = tokio_uring::uring_builder();
    ring_opts
        .setup_cqsize(opts.cq_size as _)
        // .setup_sqpoll(10)
        // .setup_sqpoll_cpu(1)
        ;

    tokio_uring::builder()
        .entries(opts.sq_size as _)
        .uring_builder(&ring_opts)
        .start(async move { black_box(Ok(())) })
}

fn run_no_ops(opts: Options) -> Result<(), Box<dyn std::error::Error>> {
    let mut ring_opts = tokio_uring::uring_builder();
    ring_opts
        .setup_cqsize(opts.cq_size as _)
        // .setup_sqpoll(10)
        // .setup_sqpoll_cpu(1)
        ;

    tokio_uring::builder()
        .entries(opts.sq_size as _)
        .uring_builder(&ring_opts)
        .start(async move {
            stream::iter(0..opts.iterations)
                .for_each_concurrent(Some(opts.concurrency), |_| async move {
                    tokio_uring::no_op().await.unwrap();
                })
                .await;
            Ok(())
        })
}

// This provides a baseline for estimating op overhead on top of this
fn no_op_x1() -> Result<(), Box<dyn std::error::Error>> {
    let opts = Options::default();
    run_no_ops(black_box(opts))
}

fn no_op_x32() -> Result<(), Box<dyn std::error::Error>> {
    let mut opts = Options::default();
    opts.concurrency = 32;
    run_no_ops(black_box(opts))
}

fn no_op_x64() -> Result<(), Box<dyn std::error::Error>> {
    let mut opts = Options::default();
    opts.concurrency = 64;
    run_no_ops(black_box(opts))
}

fn no_op_x256() -> Result<(), Box<dyn std::error::Error>> {
    let mut opts = Options::default();
    opts.concurrency = 256;
    run_no_ops(black_box(opts))
}

iai::main!(runtime_only, no_op_x1, no_op_x32, no_op_x64, no_op_x256);
