# Summary

The RFC proposes a new asynchronous Rust runtime backed by [io-uring] as a new
crate: tokio-uring. The API aims to be as close to idiomatic Tokio, but will
deviate when necessary to provide full access to io-uring's capabilities. It
also will be compatible with existing Tokio libraries. The runtime will use an
isolated thread-per-core model, and many types will be `!Send`.

[io-uring]: https://kernel.dk/io_uring.pdf?source=techstories.org

# Motivation

Tokio's current Linux implementation uses non-blocking system calls and epoll
for event notification. With epoll, a tuned TCP proxy will spend [70% to
80%][overhead] of CPU cycles outside of userspace, including cycles spent
performing syscalls and copying data between the kernel and userspace. In 2019,
Linux added a new API, io-uring, which reduces overhead by eliminating most
syscalls and mapping memory regions used for byte buffers ahead of time. Early
benchmarks comparing io-uring against epoll are promising; a TCP echo client and
server implemented in C show up to [60% improvement][bench]. Though not yet
measured, using io-uring instead of Tokio's thread-pool strategy will likely
provide significant gains for file system operations.

Because io-uring differs significantly from epoll, Tokio must provide a new set
of APIs to take full advantage of the reduced overhead. However, Tokio's
[stability guarantee][stability] means Tokio APIs cannot change until 2024 at
the earliest. Additionally, the io-uring API is still evolving with [new
functionality][tweet] planned for the near future. Instead of waiting for
io-uring to mature and a Tokio 2.0 release, we will release a standalone crate
dedicated to exposing an io-uring optimal API. This new crate will be able to
iterate rapidly with breaking changes without violating Tokio's stability
guarantee. Applications deployed exclusively on Linux kernels 5.10 or later may
choose to use this crate when taking full advantage of io-uring's benefits
provides measurable benefits. Examples of intended use-cases include TCP
proxies, HTTP file servers, and databases.

[overhead]: https://www.usenix.org/system/files/conference/nsdi14/nsdi14-paper-jeong.pdf
[bench]: https://github.com/frevib/io_uring-echo-server/blob/master/benchmarks/benchmarks.md
[stability]: https://tokio.rs/blog/2020-12-tokio-1-0#a-stability-guarantee
[tweet]: https://twitter.com/axboe/status/1371978266806919168

# Guide-level explanation

The tokio-uring crate provides a module layout similar to the Tokio crate,
including modules such as net (TCP, UDP, and Unix domain sockets), fs (file
system access), io (standard in, out, and error streams). It also provides a
runtime module containing an io-uring specific runtime. Modules such as sync and
any other containing async Rust utilities are not included by tokio-uring, in
favor of the ones provided by the main Tokio crate.

```toml
[dependencies]
tokio-uring = "0.1"
```

```rust
fn main() {
    let rt = tokio_uring::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // The rest of the application comes here.
    });
}
```

The application's `main` function starts a tokio-uring runtime and launches its
asynchronous logic within that. The tokio-uring runtime can drive both io-uring
specific resources (e.g., `TcpStream` and `TcpListener`) and Tokio resources,
enabling any library written for Tokio to run within the io-uring specific
runtime.

## Submit-based operations

Operations on io-uring backed resources return futures, representing the
operation completion. The caller awaits the future to get the operation result.

```rust
let socket = my_listener.accept().await?;
```

The runtime communicates with the kernel using two single-producer,
single-consumer queues. It submits operation requests, such as accepting a TCP
socket, to the kernel using the submission queue. The kernel then performs the
operation. On completion, the kernel returns the operation results via the
completion queue and notifies the process. The `io_uring_enter` syscall flushes
the submission queue and acquires any pending completion events. Upon request,
this syscall may block the thread waiting for a minimum number of completion
events. Both queues use memory shared between the process and the kernel and
synchronize with atomic operations.

Operation futures provide an asynchronous cancel function, enabling the caller
to await on a clean cancellation.

```rust
let accept = tcp_listener.accept();

tokio::select! {
    (tcp_stream, addr) = &mut accept => { ... }
    _ = tokio::time::sleep(Duration::from_millis(100)) => {
        // Accept timed out, cancel the in-flight accept.
        match accept.cancel().await {
            Ok(_) => { ... } // operation canceled gracefully
            Err(Completed(tcp_stream, addr)) => {
                // The operation completed between the timeout
                // and cancellation request.
            }
            Err(_) => { ... }
        }
    }
}
```

In practice, operation timeouts will deserve a dedicated API to handle the
boilerplate.

```rust
let (tcp_stream, addr) = tcp_listener
    .accept()
    .timeout(Duration::from_millis(100))
    .await?
```

The cancel and timeout function are inherent methods on operation future types.

If the operation's future drops before the operation completes, the runtime will
submit a cancellation request for the operation. However, cancellation is
asynchronous and best-effort, the operation may still complete. In that case,
the runtime discards the operation result.

The queue's single-producer characteristic optimizes for a single thread to own
a given submission queue. Supporting a multi-threaded runtime requires either
synchronizing pushing onto the submission queue or creating a queue pair per
thread. The tokio-uring runtime uses an isolated thread-per-core model,
differing from Tokio's other runtimes. Unlike Tokio's primary multi-threaded
runtime, there is no work-stealing. Tasks remain on the thread that spawned them
for the duration of their lifecycle. Each runtime thread will own a dedicated
submission and completion queue pair, and operations are submitted using the
submission queue associated with the current thread. Operation completion
futures will not implement `Send`, guaranteeing that they remain on the thread
to receive the operation result. Interestingly, the resources, e.g.,
`TcpListener`, can be `Send` as long as they do not hold operation futures
internally. It is possible to have two operations in-flight from a single
resource associated with different queues and threads.

Because operations are not `Send`, tasks awaiting these operations are also not
`Send`, making them unable to be spawned using the spawn function from the Tokio
crate. The tokio-uring crate will provide a spawn function that accepts not
`Send` tasks.

When using multiple isolated runtime threads, balancing load between them
becomes challenging. Applications must take care to ensure load remains balanced
across threads, and strategies tend to vary. For example, a TCP server can
distribute accepted connections across multiple threads, ideally while
maintaining equal load across threads. One approach is to submit "accept"
operations for the same listener concurrently across all the runtime threads
while ensuring overloaded workers back-off. Defining runtime load is out of this
crate's scope and left to the application, though pseudocode follows.

```rust
let listener = Arc::new(TcpListener::bind(...));

spawn_on_each_thread(async {
    loop {
        // Wait for this worker to have capacity. This
        // ensures there are a minimum number of workers
        // in the runtime that are flagged as with capacity
        // to avoid total starvation.
        current_worker::wait_for_capacity().await;
        
        let socket = listener.accept().await;
        
        spawn(async move { ... });
    }
})
```

## Reading and writing

Read and write operations require passing ownership of buffers to the kernel.
When the operation completes, the kernel returns ownership of the buffer to the
caller. The caller is responsible for allocating the buffer's memory and
ensuring it remains alive until the operation completes. Additionally, while the
kernel owns the memory, the process may not read from or write to the buffer. By
designing the Rust APIs using ownership passing, Rust enforces the requirements
at compile time. The following example demonstrates reading and writing with a
file resource.

```rust
use tokio_uring::buf;

/// The result of an operation that includes a buffer. The buffer must
/// be returned to the caller when the operation completes successfully
/// or fails.
///
/// This is implemented as a new type to implement std::ops::Try once
/// the trait is stabilized.
type BufResult<T> = (std::io::Result<T>, buf::Slice);

// Positional read and write function definitions
impl File {
    async fn read_at(&self, buf: buf::Slice, pos: u64) -> BufResult<usize>;
    
    async fn write_at(&self, buf: buf::Slice, pos: u64) -> BufResult<usize>;
}

/// The caller allocates a buffer
let buf = buf::IoBuf::with_capacity(4096);

// Read the first 1024 bytes of the file, when `std::ops::Try` is
// stable, the `?` can be applied directly on the `BufResult` type.
let BufResult(res, buf) = file.read_at(0, buf.slice(0..1024)).await;
// Check the result.
res?;

// Write some data back to the file.
let BufResult(res, _) = file.write_at(1024, buf.slice(0..512)).await;
res?;
```

When reading from a TCP stream, read operations remain in flight until the
socket receives data, an exchange that can take an arbitrary amount of time.
Suppose each read operation requires pre-allocating memory for the in-flight
operation. In that case, the amount of memory consumed by the process grows
linearly with the number of in-flight read operations. For applications with a
large number of open connections, this can be problematic. The io-uring API
supports registering buffer pools with the ring and configuring read operations
to use the buffer pool instead of a dedicated per-operation buffer. When the
socket receives data, the kernel checks out a buffer from the pool and returns
it to the caller. After reading the data, the caller returns the buffer to the
kernel. If no buffers are available, the operation pauses until the process
returns a buffer to the kernel, at which time the operation completes.

```rust
// ...
let buf = file.read_at_prepared(0, 1024, DefaultPool).await?;

// Write some data back to the file.
let BufResult(res, buf) = file.write_at(1024, buf.slice(0..512)).await;
res?;

// Dropping the buffer lets the kernel know the buffer may
// be reused.
drop(buf);
```

The runtime supports initializing multiple pools containing buffers of different
sizes. When submitting a read operation, the caller specifies the pool from
which to draw a buffer.

```rust
// Allocate a buffer pool
let my_pool = BufferPool::builder()
    .buffer_size(16_384)
    .num_buffers(256)
    .build();

// Create the runtime
let mut rt = tokio_uring::runtime::Runtime::new()?;

// Provide the buffer pool to the kernel. This passes
// ownership of the pool to the kernel.
let pool_token = rt.provide_buffers(my_pool)?;
    
rt.block_on(async {
    // ...
    let buf = file.read_at_prepared(0, 1024, pool_token).await?;
});
```

## Buffer management

Buffers passed to read and write operations must remain alive and pinned to a
memory location while operations are in-flight, ruling out `&mut [u8]` as an
option. Additionally, read and write operations may reference buffers using a
pointer or, when the buffer is part of a pre-registered buffer pool, using a
numeric buffer identifier. 

The tokio-uring crate will provide its buffer type, `IoBuf`, to use when reading
and writing.

```rust
pub struct IoBuf {
    kind: Kind,
}

enum Kind {
    /// A vector-backed buffer
    Vec(Vec<u8>),

    /// Buffer pool backed buffer. The pool is managed by io-uring.
    Provided(ProvidedBuf),
}
```

Internally, the IoBuf type is either backed by individually heap-allocated
memory or a buffer pool entry. Acquiring an `IoBuf` is done via
`IoBuf::with_capacity` or checking-out an entry from a pool. 

```rust
// Individually heap-allocated
let my_buf = IoBuf::with_capacity(4096);

// Checked-out from a pool
match my_buffer_pool.checkout() {
    Ok(io_buf) => ...,
    Err(e) => panic!("buffer pool empty"),
}
```

On drop, if the `IoBuf` is a buffer pool member, it is checked back in. If the
kernel initially checked out the buffer as part of a read operation, an io-uring
operation is issued to return it. Submitting the io-uring operation requires the
buffer to remain on the same thread that checked it out and is enforced by
making the `IoBuf` type `!Send`. Buffer pools will also be `!Send` as they
contain `IoBuf` values.

`IoBuf` provides an owned slice API allowing the caller to read to and write from
a buffer's sub-ranges. 

```rust
pub struct Slice {
    buf: IoBuf,
    begin: usize,
    end: usize,
}

impl IoBuf {
    fn slice(self, range: impl ops::RangeBounds<usize>) -> Slice { .. }
}

// Write a sub-slice of a buffer
my_file.write(my_io_buf.slice(10..20)).await
```

A slice end may go past the buffer's length but not past the capacity, enabling
reads to uninitialized memory.

```rust
// The buffer's memory is uninitialized
let mut buf = IoBuf::with_capacity(4096);
let slice = buf.slice(0..100);

assert_eq!(slice.len(), 0);
assert_eq!(slice.capacity(), 100);

// Read data from a file into the buffer
let BufResult(res, slice) = my_file.read_at(slice, 0);

assert_eq!(slice.len(), 100);
assert_eq!(slice.capacity(), 100);
```

A trait argument for reading and writing may be possible as a future
improvement. Consider a read API that takes `T: AsMut<[u8]>` for the buffer
argument.

```rust
async fn read<T: AsMut<[u8]>>(&self, dst: T) { ... }

struct MyUnstableBuf {
    mem: [u8; 10],
}

impl AsMut<[u8]>  for MyUnstableBuf {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.mem[..]
    }
}
```

This read function takes ownership of the buffer; however, any pointer to the
buffer obtained from a value becomes invalid when the value moves. Storing the
buffer value at a stable location while the operation is in-flight should be
sufficient to satisfy safety.

## Closing resources

Idiomatically with Rust, closing a resource is performed in the drop handler,
and within an asynchronous context, the drop handler should be non-blocking.
Closing an io-uring resource requires canceling any in-flight operations, which
is an asynchronous process. Consider an open TcpStream associated with
file-descriptor (FD) 10. A task submits a read operation to the kernel, but the
`TcpStream` is dropped and closed before the kernel sees it. A new `TcpStream`
is accepted, and the kernel reuses FD 10. At this point, the kernel sees the
original read operation request with FD 10 and completes it on the new
`TcpStream`, not the intended `TcpStream`. The runtime receives the completion
of the read operation. It discards the result because the associated operation
future is gone, resulting in the caller losing data from the second TcpStream.
This problem occurs even issuing a cancellation request for the read operation.
There is no guarantee the kernel will see the cancellation request before
completing the operation.

There are two options for respecting both requirements, neither ideal: closing
the resource in the background or blocking the thread in the drop handler. If
the resource is closed in the background, the process may encounter unexpected
errors, such as "too many open files." Blocking the thread to cancel in-flight
operations and close the resource prevents the runtime from processing other
tasks and adds latency across the system.

Instead, tokio-uring will provide an explicit asynchronous close function on
resource types.

```rust
impl TcpStream {
    async fn close(self) { ... }
}

my_tcp_stream.close().await;
```

The resource must still tolerate the caller dropping it without being explicitly
closed. In this case, tokio-uring will close the resource in the background,
avoiding blocking the runtime. The drop handler will move ownership of the
resource handle to the runtime and submit cancellation requests for any
in-flight operation. Once all existing in-flight operations complete, the
runtime will submit a close operation.

If the drop handler must process closing a resource in the background, it will
notify the developer by emitting a warning message using [`tracing`]. In the
future, it may be possible for Rust to provide a `#[must_not_drop]` attribute.
This attribute will result in compilation warnings if the developer drops a
resource without using the explicit close method.

[`tracing`]: https://github.com/tokio-rs/tracing

## Byte streams

Byte stream types, such as `TcpStream`, will not provide read and write methods
(see "Alternatives" below for reasoning). Instead, byte streams will manage
their buffers internally, as described in ["Notes on io-uring"][notes], and
implement buffered I/O traits, such as AsyncBufRead. The caller starts by
waiting for the byte stream to fill its internal buffer, reads the data, and
marks the data as consumed.

[notes]: https://without.boats/blog/io-uring/

```rust
// `fill_buf()` is provided by `AsyncBufRead`
let data: &[u8] = my_stream.fill_buf().await?;
println!("Got {} bytes", data.len());

// Consume the data
my_stream.consume(data.len());
```

Internally, byte streams submit read operations using the default buffer pool.
Additional methods exist to take and place buffers, supporting zero-copy piping
between two byte streams.

```rust
my_tcp_stream.fill_buf().await?;
let buf: IoBuf = my_tcp_stream.take_read_buf();

// Mutate `buf` if needed here.

my_other_stream.place_write_buf(buf);
my_other_stream.flush().await?;
```

Implementing buffer management on the `TcpStream` type requires tracking the
in-flight read and write operations, making the `TcpStream` type `!Send`.
Sending a `TcpStream` across threads is doable by first converting the io-uring
`TcpStream` to a standard library `TcpStream`, sending that value to a new
thread, and converting it back to an io-uring `TcpStream`.

Unlike the standard library, the `File` type does not expose a byte stream
directly. Instead, the caller requests a read or write stream, making it
possible to support multiple concurrent streams. Each file stream maintains its
cursor and issues positional read and write operations based on the cursor.

```rust
let read_stream = my_file.read_stream();
let write_stream = my_file.write_stream();

read_stream.fill_buf().await?
let buf: IoBuf = read_stream.take_read_buf();

write_stream.place_write_buf(buf);
write_stream.flush().await?;

// Because `read_stream` and `write_stream` maintain separate
// cursors, `my_file` is unchanged at the end of this example.
```

Byte streams may have a configurable number of concurrent in-flight operations.
Achieving maximum throughput [requires configuring][modern-storage] this value
to take advantage of the underlying hardware's characteristics.

[modern-storage]: https://itnext.io/modern-storage-is-plenty-fast-it-is-the-apis-that-are-bad-6a68319fbc1a

## Traits

The tokio-uring crate will not expose any traits. The crate does not aim to be a
point of abstraction for submission-based I/O models. Instead, to provide
compatibility with the existing Rust asynchronous I/O ecosystem, byte stream
types, such as `TcpStream`, will implement Tokio's [AsyncRead], [AsyncWrite],
and [AsyncBufRead] traits. Using these traits requires an additional copy
between the caller's buffer and the byte stream's internal buffer compared to
taking and placing buffers.

[AsyncRead]: https://docs.rs/tokio/1/tokio/io/trait.AsyncRead.html
[AsyncWrite]: https://docs.rs/tokio/1/tokio/io/trait.AsyncWrite.html
[AsyncBufRead]: https://docs.rs/tokio/1/tokio/io/trait.AsyncBufRead.html

# Implementation details

Creating the tokio-uring runtime initializes the io-uring submission and
completion queues and a Tokio current-thread epoll-based runtime. Instead of
waiting on completion events by blocking the thread on the io-uring completion
queue, the tokio-uring runtime registers the completion queue with the epoll
handle. By building tokio-uring on top of Tokio's runtime, existing Tokio
ecosystem crates can work with the tokio-uring runtime.

When the kernel pushes a completion event onto the completion queue,
"epoll_wait" unblocks and returns a readiness event. The Tokio current-thread
runtime then polls the io-uring driver task, draining the completion queue and
notifying completion futures. 

Like Tokio, using an I/O type does not require explicitly referencing the
runtime. Operations access the current runtime via a thread-local variable.
Requiring a handle would be intrusive as the application must pass the handle
throughout the code. Additionally, an explicit handle would require a
pointer-sized struct, causing binary bloat. The disadvantage with such an
approach is that there is no way to guarantee operation submission happens
within the runtime context at compile time. Attempting to use a tokio-uring
resource from outside of the runtime will result in a panic.

```rust
use tokio_uring::runtime::Runtime;
use tokio_uring::net::TcpListener;

fn main() {
    // Binding a TcpListener does not require access to the runtime.
    let listener = TcpListener::bind("0.0.0.0:1234".parse().unwrap());

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        // This works, as `block_on` sets the thread-local variable.
        let _ = listener.accept().await;
    });

    // BOOM: panics because called outside of the runtime
    futures::future::block_on(async {
        let _ = listener.accept().await;
    });
}
```

## Operation state

Most io-uring operations reference resources, such as buffers and file
descriptors, for the kernel to use. These resources must remain available while
the operation is in-flight. Any memory referenced by pointers must remain
allocated, and the process must not access the memory. Because asynchronous Rust
allows dropping futures at any time, the operation futures may not own data
referenced by the in-flight operation. The tokio-uring runtime will take
ownership and store resources referenced by operations while they are in-flight.

```rust
struct IoUringDriver {
    // Storage for state referenced by in-flight operations
    in_flight_operations: Slab<Operation>,
    
    // The io-uring submission and completion queues.
    queues: IoUringQueues,
}

struct Operation {
    // Resources referenced by the kernel, this must stay
    // available until the operation completes.
    state: State,
    lifecycle: Lifecycle,
}

enum State {
    // An in-flight read operation, `None` when reading into
    // a kernel-owned buffer pool
    Read { buffer: Option<buf::Slice> },
    Write { buffer: buf::Slice },
    // Accept a TCP socket
    Accept { ... }
    Close { ... }
    // ... other operations
}

enum Lifecycle {
    /// The operation has been submitted to uring and is currently in-flight
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result.
    Ignored,

    /// The operation has completed. The completion result is stored. 
    Completed(Completion),
}

// Completion result returned from the kernel via the completion queue
struct Completion {
    result: io::Result<u32>,
    flags: u32,
}
```

The `Operation` struct holds any data referenced by the operation submitted to
the kernel, preventing the data from being dropped early. The lifecycle field
tracks if the operation is in-flight, has completed, or if the associated future
has dropped. The lifecycle field also passes operation results from the driver
to the future via the `Completed` variant.

When a task starts a read operation, the runtime allocates an entry in the
in-flight operation store, storing the buffer and initializing the lifecycle to
`Submitted`. The runtime then pushes the operation to the submission queue but
does not synchronize it with the kernel. Synchronization happens once the task
yields to the runtime, enabling the task to submit multiple operations without
synchronizing each one. Delaying synchronization can add a small amount of
latency. While not enforced, tasks should execute for no more than 500
microseconds before yielding.

When the runtime receives completion results, it must complete the associated
operation future. The runtime loads the in-flight operation state, stores the
result, transitions the lifecycle to `Completed`, and notifies the waker. The
next time the caller's task executes, it polls the operation's future which
completes and returns the stored result.

If the operation's future drops before the operation completes and the operation
request is still in the submission queue, the drop function removes the request.
Otherwise, it sets the lifecycle to `Ignored` and submits a cancellation request
to the kernel. The cancellation request will attempt to terminate the operation,
causing it to complete immediately with an error. Cancellation is best-effort;
the operation may or may not terminate early. If the operation does complete,
the runtime discards the result. The runtime maintains the internal operation
state until the completion as this state owns data the kernel may be
referencing.

## Read operations

Depending on the flavor, read operation can have multiple io-uring operation
representations. The io-uring API provides two different read operation codes:
`IORING_OP_READ` and `IORING_OP_READ_FIXED`. The first accepts a buffer as a
pointer, and the second takes a buffer as an identifier referencing a buffer
pool and entry. Additionally, the `IORING_OP_READ` operation can accept a null
buffer pointer, indicating that io-uring should pick a buffer from a provided
buffer pool. The tokio-uring runtime will determine which opcode to use based on
the kind of `IoBuf` kind it receives. The `Vec` kind maps to the `IORING_OP_READ`
opcode, and the `Provided` maps to `IORING_OP_READ_FIXED`.

## Prior art

[Glommio] is an existing asynchronous Rust runtime built on top of io-uring.
This proposal draws heavily from ideas presented there, tweaking concepts to
line up with Tokio's idioms. [`@withoutboats`][boats] has explored the space
with [ringbahn].

The tokio-uring crate is built on the pure-rust [io-uring] crate, authored by
[@quininer]. This crate provides a low-level interface to the io-uring syscalls.

[Glommio]: https://github.com/DataDog/glommio
[boats]: https://github.com/withoutboats
[ringbahn]: https://github.com/ringbahn/ringbahn
[@quininer]: https://github.com/quininer/
[io-uring]: https://github.com/tokio-rs/io-uring/

## Future work

The main Tokio crate will likely adopts concepts from tokio-uring in the future.
The most obvious area is Tokio's file system API, currently implemented using a
thread-pool. The current API would remain, and the implementation would use
io-uring when supported by the operating system. The tokio-uring APIs may form
the basis for a Tokio 2.0 release, though this cannot happen until 2024 at the
earliest. As an intermittent step, the tokio-uring crate could explore
supporting alternate backends, such as epoll, kqueue, and iocp. The focus will
always remain on io-uring.

The current design does not cover registering file descriptors with the kernel,
which improves file system access performance. After registering a file
descriptor with io-uring, it must not move to a different thread, implying
`!Send`. Because many resource types are Send, a likely path for supporting the
feature is adding new types to represent the registered state. For example,
`File` could have a `RegisteredFile` analog. Making this change would be
forwards compatible and not impact the current design.

## Alternatives

### Use a work-stealing scheduler

An alternative approach could use a work-stealing scheduler, allowing
underutilized worker threads to steal work from overloaded workers. While
work-stealing is an efficient strategy for balancing load across worker threads,
required synchronization adds overhead. [An earlier article] on the Tokio blog
includes an overview of various scheduling strategies.

The tokio-uring crate targets use-cases that can benefit from taking advantage
of io-uring at the expense of discarding Tokio's portable API. These use cases
will also benefit from reduced synchronization overhead and fine-grained control
over thread load balancing strategies.

[An earlier article]: https://tokio.rs/blog/2019-10-scheduler

### Read and write methods on TcpStream

Read and write operations on byte stream types, such as `TcpStream`, are
stateful. Each operation operates on the next chunk of the stream, advancing a
logical cursor. The combination of submit-based operations with drop-to-cancel
semantics presents a challenge. Consider the following.

```rust
let buf = IoBuf::with_capacity(4096);

// This read will be canceled.
select! {
    _ = my_tcp_stream.read(buf.slice(..)) => unreachable!(),
    _ = future::ready(()) => {}
}

let buf = IoBuf::with_capacity(4096);
let (res, buf) = my_tcp_stream.read(buf.slice(..)).await;
```

Dropping the first read operation cancels it, and because it never completes,
the second read operation should read the first packet of data from the TCP
stream. However, the kernel may have already completed the first read operation
before seeing the cancellation request. A naive runtime implementation would
drop the completion result of any canceled operation, which would result in
losing data. Instead, the runtime could preserve the data from the first read
operation and return it as part of the second read. The process would not lose
data, but the runtime would need to perform an extra copy or return the caller a
different buffer than the one it submitted.


The proposed API is not vulnerable to this issue as resources track their
operations, preventing them from being dropped as long as the resource is open.

### Expose a raw io-uring operation submission API

The proposed tokio-uring API does not include a strategy for the user to submit
custom io-uring operations. Any raw API would be unsafe or would require the
runtime to support taking opaque data as a trait object. Given that io-uring has
a well-defined set of supported options, tokio-uring opts to support each
operation explicitly. The application also can create its own set of io-uring
queues using the io-uring crate directly.

### Load-balancing spawn function

The tokio-uring crate omits a spawn function that balances tasks across threads,
leaving this to future work. Instead, the application should manage its load
balancing. While there are a few common balancing strategies, such as
round-robin, randomized, and power of two choices, there is no one-size-fits-all
strategy. Additionally, some load-balancing methods require application-specific
metrics such as worker load.

Additionally, consider pseudocode representing a typical accept loop pattern.

```rust
loop {
    let (socket, _) = listener.accept().await?;

    spawn(async move {
        let request = read_request(&socket).await;
        let response = process(request).await;
        write_response(response, &socket).await;
    });
}
```

Often, the request data is already buffered when the socket is accepted. If the
spawn call results in the task moving to a different worker thread, it will
delay reading the request due to cross-thread synchronization. Additionally, it
may be possible to batch an accept operation with an operation that reads from
the accepted socket in the future, complicating moving the accepted socket to a
new thread. The details of such a strategy are still not finalized but may
impact a load-balancing spawn function.
