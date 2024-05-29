//! An UDP multicast example with a client (receiver) and server (sender).
//! It sends the numbers from 1 to 10 in separate packets.
//!
//! You can test this out by in one terminal executing:
//!
//!     cargo run --example udp_multicast client
//!
//! and in another terminal you can run:
//!
//!     cargo run --example udp_multicast server

use socket2::{Protocol, Socket, Type};
use std::env;
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio_uring::net::UdpSocket;

fn run_server() -> std::io::Result<()> {
    tokio_uring::start(async {
        let std_addr: SocketAddrV4 = "0.0.0.0:0".parse().unwrap();
        let mc_addr: SocketAddrV4 = "239.0.0.1:2401".parse().unwrap();
        let sock = Socket::new(socket2::Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        sock.set_reuse_port(true)?;
        sock.set_nonblocking(true)?;
        sock.bind(&std_addr.into())?;

        let std_socket = UdpSocket::from_std(sock.into());

        // write data
        for i in 1..=10 {
            let (result, _) = std_socket
                .send_to(i.to_string().as_bytes().to_owned(), mc_addr.into())
                .await;
            result.unwrap();
        }
        Ok(())
    })
}

fn run_client() -> std::io::Result<()> {
    tokio_uring::start(async {
        let mc_addr: SocketAddrV4 = "239.0.0.1:2401".parse().unwrap();
        let sock = Socket::new(socket2::Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        sock.set_reuse_port(true)?;
        sock.set_nonblocking(true)?;
        sock.bind(&mc_addr.into())?;
        sock.join_multicast_v4(mc_addr.ip(), &Ipv4Addr::UNSPECIFIED)?;

        let std_socket = UdpSocket::from_std(sock.into());

        // read data
        for i in 1..=10 {
            let buf = vec![0; 128];

            let (result, buf) = std_socket.recv_from(buf).await;
            let (n_bytes, _) = result.unwrap();

            assert_eq!(i.to_string().as_bytes(), &buf[..n_bytes]);
        }
        Ok(())
    })
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("unkown mode, enter 'client' or 'server' as argument");
    } else if &args[1] == "server" {
        return run_server();
    } else if &args[1] == "client" {
        return run_client();
    } else {
        println!("unkown mode, enter arg 'client' or 'server' as argument");
    }
    Ok(())
}
