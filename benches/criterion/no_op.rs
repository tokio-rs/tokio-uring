/// Benchmark the overhead of the uring no_op operation.
///
/// It should be understood that a single call to even the no_op operation is relatively expensive
/// even though the kernel is not being asked to do anything except copy the user_data from the
/// submission queue entry to a completion queue entry.
///
/// The cost comes from the application creating writing to the SQE, then just before going idle,
/// making the system call to notify the kernel that the submission queue tail has been changed by
/// the application, then being awoken at the mio level by the kernel that something about the
/// uring's fd has become ready, then running through the tokio_uring's completion ring handler and
/// finding the CQE that trigger's a lookup into the slab that gets a future's awake function
/// called, that makes a task runnable, that then allows the Tokio runtime to schedule the task
/// that finally gets to see the no_op operation completed.
///
/// When run in isolation, that is to say when then concurrency parameter is 1, this round trip
/// takes between 3.2 and 1.5 *micro* seconds on a 64 bit Linux machine - depending on the CPU
/// speed and the cache sizes.
///
/// When run with enough concurrency, say 1000 tasks each doing the same thing, it can be seen the
/// actual overhead for the tokio_uring no_op call and wait comes down to between 310 and 220ns,
/// again depending on CPU and cache details. Given that a trivial yield by a Tokio task itself
/// takes generally 100ns, this overhead for a uring operation is not much.
///
/// It just takes a lot of concurrent work going through the uring interface to see that benefit.
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode};
use std::time::{Duration, Instant};

use tokio::task::JoinSet;

#[derive(Clone)]
struct Options {
    // Variable, per benchmark.
    concurrency: u64,

    // Constants, same for all benchmarks.
    sq_size: usize,
    cq_size: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            // Default to 1, but each benchmark actually sets the concurrenty level.
            concurrency: 1,

            // We run up to 10K concurrent tasks in this benchmark, but optimize for 1K, so create
            // an input ring of the same size. We understand 1,000 would be rounded up to 1024, so
            // use that size explicitly anyway.
            sq_size: 1024,

            // The completion ring is going to be double the submission ring by default, so this
            // wouldn't be necessary unless we want to experiment with larger multiples.
            cq_size: 2 * 1024,
        }
    }
}

fn run_no_ops(opts: &Options, count: u64) -> Duration {
    let mut ring_opts = tokio_uring::uring_builder();
    ring_opts.setup_cqsize(opts.cq_size as _);

    tokio_uring::builder()
        .entries(opts.sq_size as _)
        .uring_builder(&ring_opts)
        .start(async move {
            let mut js = JoinSet::new();

            // Prepare the number of concurrent tasks this benchmark calls for.
            // They will each be blocked until the current task yields the thread
            // when it calls await on the first join_next below.
            //
            // Within each task, run through the number of no_op calls in series based on the
            // benchmark count (accounting for the number of tasks that are being spawned).

            for _ in 0..opts.concurrency {
                let opts = opts.clone();
                js.spawn_local(async move {
                    // Run the required number of iterations (per the concurrency amount)
                    for _ in 0..(count / opts.concurrency) {
                        let _ = black_box(tokio_uring::no_op().await);
                    }
                });
            }

            // Measure from the moment we start waiting for the futures.
            let start = Instant::now();

            while let Some(res) = js.join_next().await {
                res.unwrap();
            }

            // Return the elapsed time.
            start.elapsed()
        })
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("no_op");
    let mut opts = Options::default();
    for concurrency in [1, 2, 5, 10, 100, 1000, 10_000].iter() {
        opts.concurrency = *concurrency;

        // We perform long running benchmarks: this is the best mode
        group.sampling_mode(SamplingMode::Flat);

        group.bench_with_input(
            BenchmarkId::new("concurrent tasks", concurrency),
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
