// An example of an echo server using fixed buffers for reading and writing TCP streams.
// A buffer registry size of two is created, to allow a maximum of two simultaneous connections.

use std::{env, iter, net::SocketAddr};

use tokio_uring::{
    buf::{fixed::FixedBufRegistry, BoundedBuf, IoBufMut},
    net::{TcpListener, TcpStream},
}; // BoundedBuf for slice method

// A contrived example, where just two fixed buffers are created.
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

// Bind to address and accept connections, spawning an echo handler for each connection.
async fn accept_loop(listen_addr: SocketAddr) {
    let listener = TcpListener::bind(listen_addr).unwrap();

    println!(
        "Listening on {}, fixed buffer pool size only {POOL_SIZE}",
        listener.local_addr().unwrap()
    );

    // Other iterators may be passed to FixedBufRegistry::new also.
    let registry = FixedBufRegistry::new(iter::repeat(vec![0; 4096]).take(POOL_SIZE));

    // Register the buffers with the kernel, asserting the syscall passed.

    registry.register().unwrap();

    loop {
        let (stream, peer) = listener.accept().await.unwrap();

        tokio_uring::spawn(echo_handler(stream, peer, registry.clone()));
    }
}

// A loop that echoes input to output. Use one fixed buffer for receiving and sending the response
// back. Once the connection is closed, the function returns and the fixed buffer is dropped,
// getting the fixed buffer index returned to the available pool kept by the registry.
async fn echo_handler<T: IoBufMut>(
    stream: TcpStream,
    peer: SocketAddr,
    registry: FixedBufRegistry<T>,
) {
    println!("peer {} connected", peer);

    // Get one of the two fixed buffers.
    // If neither is unavailable, print reason and return immediately, dropping this connection;
    // be nice and shutdown the connection before dropping it so the client sees the connection is
    // closed immediately.

    let mut fbuf = registry.check_out(0);
    if fbuf.is_none() {
        fbuf = registry.check_out(1);
    };
    if fbuf.is_none() {
        let _ = stream.shutdown(std::net::Shutdown::Write);
        println!("peer {} closed, no fixed buffers available", peer);
        return;
    };

    let mut fbuf = fbuf.unwrap();

    let mut n = 0;
    loop {
        // Each time through the loop, use fbuf and then get it back for the next
        // iteration.

        let (read, fbuf1) = stream.read_fixed(fbuf).await.unwrap();
        fbuf = {
            if read == 0 {
                break;
            }
            assert_eq!(4096, fbuf1.len()); // To prove a point.

            let (_, nslice) = stream.write_fixed_all(fbuf1.slice(..read)).await.unwrap();
            println!("peer {} all {} bytes ping-ponged", peer, read);
            n += read;

            // Important. One of the points of this example.
            nslice.into_inner() // Return the buffer we started with.
        };
    }
    let _ = stream.shutdown(std::net::Shutdown::Write);
    println!("peer {} closed, {} total ping-ponged", peer, n);
}
