pub mod probe;

use std::net::SocketAddr;

use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio_uring::buf::bufring;
use tokio_uring::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub enum Rx {
    Read,
    Recv,
    RecvBufRing(bufring::BufRing),
}

pub async fn tcp_listener() -> Result<(TcpListener, SocketAddr), std::io::Error> {
    let socket_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let listener = TcpListener::bind(socket_addr).unwrap();

    let socket_addr = listener.local_addr().unwrap();

    Ok((listener, socket_addr))
}

async fn client_ping_pong(rx: Rx, stream: &TcpStream, send_cnt: usize, send_length: usize) {
    // Implement client ping-pong loop. Make several read variations available.

    for _ in 0..send_cnt {
        // Make this vector longer to cause splits in round trip transmission.
        let buf = vec![1u8; send_length];

        let (result, buf) = stream.write_all(buf).await;
        let _result = result.unwrap();
        // println!("client: written: {}", _result);

        let expect = buf.len();
        let mut got: usize = 0;

        // println!("client: buf.len {}", buf.len());
        let mut buf = buf;
        while got < expect {
            let result;

            result = match &rx {
                Rx::Read => {
                    let result;
                    (result, buf) = stream.read(buf).await;
                    result
                }
                Rx::Recv => {
                    let result;
                    (result, buf) = stream.recv(buf).await;
                    result
                }
                Rx::RecvBufRing(group) => {
                    loop {
                        let buf = stream.recv_provbuf(group.clone()).await;
                        match buf {
                            Ok(buf) => {
                                // If returning a Vec<u8> were necessary:
                                //  Either form of conversion from Bufx data to Vec<u8> could be appropriate here.
                                //  One consumes the BufX, the other doesn't and let's it drop here.
                                // break (Ok(buf.len()), buf.into())
                                // break (Ok(buf.len()), buf.as_slice().to_vec());
                                break Ok(buf.len());
                            }
                            Err(e) => {
                                // Expected error: No buffer space available (os error 105)
                                // but sometimes getting error indicating the returned res was 0
                                // and flags was 4.
                                if e.kind() == std::io::ErrorKind::Other {
                                    eprintln!(
                                        "client: assuming connection is closed: ecv_provbuf error {}",
                                        e
                                    );
                                    break Err(e);
                                }
                                eprintln!("client: recv_provbuf error {}", e);
                            }
                        }
                    }
                }
            };
            let read = result.unwrap();
            got += read;
            // level1-println!("client: read {}", read);
            // println!("client: read: {:?}", &_buf[..read]);
        }
    }
}

async fn server_ping_pong_reusing_vec(
    rx: Rx,
    stream: TcpStream,
    buf: Vec<u8>,
    _local_addr: SocketAddr,
) {
    use tokio_uring::buf::BoundedBuf; // for slice()

    let mut buf = buf;
    // level1-println!("server: {} connected", peer);
    let mut _n = 0;

    loop {
        let (result, nbuf) = match &rx {
            Rx::Read => stream.read(buf).await,
            Rx::Recv => stream.recv(buf).await,
            Rx::RecvBufRing(_) => unreachable!(),
        };
        buf = nbuf;
        let read = result.unwrap();
        if read == 0 {
            // level1-println!("server: {} closed, {} total ping-ponged", peer, _n);
            break;
        }

        let (res, slice) = stream.write_all(buf.slice(..read)).await;
        let _ = res.unwrap();
        buf = slice.into_inner();
        // level1-println!("server: {} all {} bytes ping-ponged", peer, read);
        _n += read;
    }
}

async fn server_ping_pong_using_buf_ring(
    stream: TcpStream,
    group: &bufring::BufRing,
    _local_addr: SocketAddr,
) {
    // Serve the connection by looping on input, each received bufx from the kernel which
    // we let go out of scope when we are done so it can be given back to the kernel.
    //
    // Here is a completion model based loop, as described in
    // https://github.com/axboe/liburing/wiki/io_uring-and-networking-in-2023
    // where the buffer being written to by the kernel is picked by the kernel from a
    // provided buffer pool, and when finished working with the buffer, it is returned to
    // the kernel's provided buffer pool.

    let mut _n = 0;
    loop {
        // Loop to allow trying again if there was no buffer space available.
        let bufx = loop {
            let buf = stream.recv_provbuf(group.clone()).await;
            match buf {
                Ok(buf) => break buf,
                Err(e) => {
                    // Expected error: No buffer space available (os error 105),
                    // for which we loop around.
                    //
                    // But sometimes getting error indicating the returned res was 0
                    // and flags was 4. Treat this like the connection is closed while
                    // awaiting confirmation from the io_uring team.
                    if e.kind() == std::io::ErrorKind::Other {
                        eprintln!(
                            "server: assuming connection is closed: ecv_provbuf error {}",
                            e
                        );
                        return;
                    }
                    eprintln!("server: recv_provbuf error {}", e);
                }
            }
        };

        // Copy of logic above, but different buffer type.

        let read = bufx.len();
        if read == 0 {
            // Unlikely, as the zero case seems handled by the error above.
            break;
        }

        // Writing bufx or bufx.slice(..read) works equally well as the bufx length *is*
        // the length that was read.
        // let (res, _) = stream.write_all(bufx.slice(..read)).await;
        // let (res, _) = stream.write_all(bufx).await;
        //
        // The returned _ represents the BufX or slice of the BufX which we let go out of scope.

        let (res, _) = stream.write_all(bufx).await;

        let _ = res.unwrap();
        // level1-println!("server: {} all {} bytes ping-ponged with bufx", peer, read);
        _n += read;
    }
}

pub async fn async_block_ping_pong_listener_loop(server: Server, listener: TcpListener) {
    let Server { rx } = server;
    loop {
        let (stream, socket_addr) = listener.accept().await.unwrap();
        let rx = rx.clone();

        // Spawn new task for each connnection
        tokio_uring::spawn(async move {
            match &rx {
                Rx::Read | Rx::Recv => {
                    let buf = vec![0u8; 16 * 1024];
                    server_ping_pong_reusing_vec(rx, stream, buf, socket_addr).await;
                }
                Rx::RecvBufRing(group) => {
                    server_ping_pong_using_buf_ring(stream, group, socket_addr).await;
                }
            };
        });
    }
}

fn spawn_ping_pong_listener_loop(server: Server, listener: TcpListener) {
    tokio_uring::spawn(async move {
        async_block_ping_pong_listener_loop(server, listener).await;
    });
}

pub fn ping_pong_clients(clients: Clients, listener_addr: SocketAddr) -> oneshot::Receiver<()> {
    // Spawn clients as tokio_uring tasks and return a tokio oneshot receiver
    // that will indicate when they are done.

    let Clients {
        rx,
        client_cnt,
        send_cnt,
        send_length,
    } = clients;

    let mut set = JoinSet::new();

    // Spawn several clients

    for client_id in 0..client_cnt {
        let rx = rx.clone();
        set.spawn_local(async move {
            let stream = TcpStream::connect(listener_addr).await.unwrap();
            client_ping_pong(rx, &stream, send_cnt, send_length).await;

            client_id // return through handle
        });
    }

    let (tx, rx) = oneshot::channel::<()>();

    tokio_uring::spawn(async move {
        let mut seen = vec![false; client_cnt];
        while let Some(res) = set.join_next().await {
            let client_id = res.unwrap();
            seen[client_id] = true;
        }

        for i in 0..client_cnt {
            assert!(seen[i]);
        }
        let _ = tx.send(()).unwrap();
    });

    rx
}

async fn _ping_pong(clients: Clients, server: Server) {
    // Run `client_cnt` clients. Both clients and server use the TcpStream method identified by `rx`.

    let (listener, listener_addr) = tcp_listener().await.unwrap();

    // Spawn perhaps multiple clients

    let clients_done = ping_pong_clients(clients, listener_addr);

    // Spawn one server

    spawn_ping_pong_listener_loop(server, listener);

    // Wait until the clients signal they are done

    // println!("common/mode.rs:{} now wait for clients to be done", line!());
    let _ = clients_done.await.unwrap();
    // println!("common/mode.rs:{} clients report being done", line!());
}

pub struct Clients {
    pub rx: Rx,
    pub client_cnt: usize,
    pub send_cnt: usize,
    pub send_length: usize,
}

pub struct Server {
    pub rx: Rx,
}

pub struct PingPong {
    pub clients: Clients,
    pub server: Server,
}

impl PingPong {
    pub async fn run(self) {
        let PingPong { clients, server } = self;
        _ping_pong(clients, server).await;
    }
}
