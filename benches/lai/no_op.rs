use iai::black_box;
use tokio::task::JoinSet;

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
    ring_opts.setup_cqsize(opts.cq_size as _);

    tokio_uring::builder()
        .entries(opts.sq_size as _)
        .uring_builder(&ring_opts)
        .start(async move { black_box(Ok(())) })
}

fn run_no_ops(opts: Options) -> Result<(), Box<dyn std::error::Error>> {
    let mut ring_opts = tokio_uring::uring_builder();
    ring_opts.setup_cqsize(opts.cq_size as _);

    tokio_uring::builder()
        .entries(opts.sq_size as _)
        .uring_builder(&ring_opts)
        .start(async move {
            let mut js = JoinSet::new();

            for _ in 0..opts.iterations {
                js.spawn_local(tokio_uring::no_op());
            }

            while let Some(res) = js.join_next().await {
                res.unwrap().unwrap();
            }

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
