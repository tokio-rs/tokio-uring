use std::{env, net::SocketAddr};

use tokio_uring::net::TcpListener;

fn main() {
    let args: Vec<_> = env::args().collect();

    let socket_addr = if args.len() <= 1 {
        "127.0.0.1:0"
    } else {
        args[1].as_ref()
    };
    let socket_addr: SocketAddr = socket_addr.parse().unwrap();

    tokio_uring::start(async {
        let listener = TcpListener::bind(socket_addr).unwrap();

        println!("Listening on {}", listener.local_addr().unwrap());

        loop {
            let (stream, peer_addr) = listener.accept().await.unwrap();
            tokio_uring::spawn(async move {
                // implement ping-pong loop

                use tokio_uring::buf::IoBuf; // for slice()

                println!("{} connected", peer_addr);
                let mut tot = 0;

                let mut buf = vec![0u8; 4096];
                loop {
                    let (result, nbuf) = stream.read(buf).await;
                    buf = nbuf;
                    let read = result.unwrap();
                    if read == 0 {
                        println!("{} closed, {} total bytes ping-ponged\n\n", peer_addr, tot);
                        break;
                    }
                    tot += read;

                    let (res, slice) = stream.write_all(buf.slice(..read)).await;
                    let _ = res.unwrap();
                    buf = slice.into_inner();
                    println!(
                        "{} all {} bytes ping-ponged, running total {} bytes",
                        peer_addr, read, tot
                    );
                }
            });
        }
    });
}
