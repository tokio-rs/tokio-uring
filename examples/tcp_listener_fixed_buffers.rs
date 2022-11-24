// An example of using fixed buffers for reading and writing with TCP streams.
// A buffer registry size of two is created, to allow a maximum of two simultaneous connections.

use std::{env, iter, net::SocketAddr};

use tokio_uring::buf::fixed::FixedBufRegistry;
use tokio_uring::buf::BoundedBuf;
use tokio_uring::net::{TcpListener, TcpStream}; // for slice method

// A bit contrived example, where just two fixed buffers are created.
const POOL_SIZE: usize = 2;

fn main() {
    let args: Vec<_> = env::args().collect();

    let socket_addr = if args.len() <= 1 {
        "127.0.0.1:0"
    } else {
        args[1].as_ref()
    };
    let socket_addr: SocketAddr = socket_addr.parse().unwrap();

    tokio_uring::start(accept_loop(socket_addr));
}

// Bind to address and accept connections, spawning a ping_pong handler for each connection.
async fn accept_loop(listen_addr: SocketAddr) {
    let listener = TcpListener::bind(listen_addr).unwrap();

    println!(
        "Listening on {}, fixed buffer pool size only {POOL_SIZE}",
        listener.local_addr().unwrap()
    );

    let registry = FixedBufRegistry::new(iter::repeat(vec![0; 4096]).take(POOL_SIZE));

    // Register the buffers with the kernel, asserting the syscall passed.

    registry.register().unwrap();

    loop {
        let (stream, peer) = listener.accept().await.unwrap();

        tokio_uring::spawn(ping_pong(stream, peer, registry.clone()));
    }
}

// Implement ping-pong loop.
// Use one fixed buffer for receiving and sending response back.
// Drop the fixed buffer once the connection is closed.
async fn ping_pong(stream: TcpStream, peer: SocketAddr, registry: FixedBufRegistry) {
    println!("{} connected", peer);

    // Try to get fixed buffer at index 0. If unavailable, try with index 1.
    // If both are unavailable, print reason and return immediately, dropping this connection.

    let mut fbuf = registry.check_out(0);
    if fbuf.is_none() {
        fbuf = registry.check_out(1);
    };
    if fbuf.is_none() {
        println!("No further fixed buffers available.");
        return;
    };

    let mut fbuf = fbuf.unwrap();

    let mut n = 0;
    loop {
        // Each time through the loop, use fbuf and then get it back for the next
        // iteration.

        let (result, fbuf1) = stream.read_fixed(fbuf).await;
        fbuf = {
            let read = result.unwrap();
            if read == 0 {
                break;
            }
            assert_eq!(4096, fbuf1.len()); // To prove a point.

            // Ignore fact that this isn't a write_all yet. My first tests are with
            // short texts from nc anyway.

            let (res, nslice) = stream.write_fixed(fbuf1.slice(..read)).await;

            let _ = res.unwrap();
            println!("{} all {} bytes ping-ponged", peer, read);
            n += read;

            // Important. One of the points of this example.
            nslice.into_inner() // Return the buffer we started with.
        };
    }
    println!("{} closed, {} total ping-ponged", peer, n);
}
