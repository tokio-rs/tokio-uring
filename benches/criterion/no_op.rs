use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use std::time::{Duration, Instant};

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
            sq_size: 128,
            cq_size: 256,
        }
    }
}

fn run_no_ops(opts: &Options, count: u64) -> Duration {
    let mut ring_opts = tokio_uring::uring_builder();
    ring_opts.setup_cqsize(opts.cq_size as _);

    let mut m = Duration::ZERO;

    // Run the required number of iterations
    for _ in 0..count {
        m += tokio_uring::builder()
            .entries(opts.sq_size as _)
            .uring_builder(&ring_opts)
            .start(async move {
                let mut js = JoinSet::new();

                for _ in 0..opts.iterations {
                    js.spawn_local(tokio_uring::no_op());
                }

                let start = Instant::now();

                while let Some(res) = js.join_next().await {
                    res.unwrap().unwrap();
                }

                start.elapsed()
            })
    }
    m
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("no_op");
    let mut opts = Options::default();
    for concurrency in [1, 32, 64, 256].iter() {
        opts.concurrency = *concurrency;

        // We perform long running benchmarks: this is the best mode
        group.sampling_mode(SamplingMode::Flat);

        group.throughput(Throughput::Elements(opts.iterations as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            &opts,
            |b, opts| {
                // Custom iterator used because we don't expose access to runtime,
                // which is required to do async benchmarking with criterion
                b.iter_custom(move |iter| run_no_ops(opts, iter));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
