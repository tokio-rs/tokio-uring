use std::{env, net::SocketAddr};

use tokio_uring::net::TcpStream;

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() <= 1 {
        panic!("no addr specified");
    }

    let socket_addr: SocketAddr = args[1].parse().unwrap();

    tokio_uring::start(async {
        let stream = TcpStream::connect(socket_addr).await.unwrap();
        let buf = vec![1u8; 128];

        let (written, buf) = stream.write(buf).submit().await.unwrap();
        println!("written: {}", written);

        let (read, buf) = stream.read(buf).await.unwrap();
        println!("read: {:?}", &buf[..read]);
    });
}
