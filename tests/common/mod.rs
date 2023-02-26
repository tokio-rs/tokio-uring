pub mod probe;

use std::net::SocketAddr;

use tokio::io::{self, Error, ErrorKind};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time;
use tokio_stream::StreamExt;
use tokio_uring::buf::bufring;
use tokio_uring::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub enum Rx {
    Read(ReadProps),
    Recv(RecvProps),
    RecvBufRing(BufRingProps),
    RecvMulti(BufRingProps),
}

#[derive(Clone)]
pub struct ReadProps {
    pub buf_size: usize,
}

#[derive(Clone)]
pub struct RecvProps {
    pub buf_size: usize,
    pub flags: Option<i32>,
}

#[derive(Clone)]
pub struct BufRingProps {
    pub buf_ring: bufring::BufRing,
    pub flags: Option<i32>,
    pub quiet_overflow: bool,
}

pub async fn tcp_listener() -> Result<(TcpListener, SocketAddr), std::io::Error> {
    let socket_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let listener = TcpListener::bind(socket_addr).unwrap();

    let socket_addr = listener.local_addr().unwrap();

    Ok((listener, socket_addr))
}

#[inline]
pub fn is_no_buffer_space_available(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(105)
}
#[inline]
pub fn is_connection_reset_by_peer(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(104)
}
#[inline]
pub fn is_broken_pipe(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(32)
}

async fn _client_ping_pong_recv_multi(
    stream: TcpStream,
    send_cnt: usize,
    send_buf: Vec<u8>,
    props: &BufRingProps,
) -> io::Result<()> {
    // Send one recv_multi, and then some number of write_all operations.
    let BufRingProps {
        buf_ring,
        flags,
        quiet_overflow,
    } = props;
    let mut send_buf = send_buf;
    let mut sent: usize = 0;
    let expect = send_buf.len();
    let mut got: usize = expect;

    // Invert the normal loop counters in order to perform the multishot recv at the top.
    while sent < send_cnt || got < expect {
        let buffers = stream.recv_multi(buf_ring.clone(), *flags);
        tokio::pin!(buffers);

        'new_stream: while sent < send_cnt || got < expect {
            if got > expect {
                return Err(Error::new(
                    ErrorKind::Other,
                    format!("client read got {got}, more than expected of {expect}"),
                ));
            }
            if got == expect {
                got = 0;
                sent += 1;
                let result;
                (result, send_buf) = stream.write_all(send_buf).await;
                let _ = result?;
            }

            // Keep receiving until we've gotten enough.
            while let Some(b) = buffers.next().await {
                let bufx = match b {
                    Ok(buf) => buf,
                    Err(e) => {
                        if is_no_buffer_space_available(&e) {
                            if !quiet_overflow {
                                println!("client: sleep 1us, recoverable recv_multi error {}", e);
                            }
                            time::sleep(time::Duration::from_micros(1)).await;
                            // Have a stream restarted with a new read.
                            break 'new_stream;
                        }
                        return Err(Error::new(ErrorKind::Other,
                                    format!("client with send {sent}, got {got}, expect {expect}, sees recv_multi error {e}")));
                    }
                };

                let read = bufx.len();
                got += read;
                if got >= expect {
                    continue 'new_stream;
                }
            }
            // The buffer stream returned None, indicating the command finished with result (length) 0.
            println!("line {}: buffer stream ended: client has sent {sent} packets and got {got} bytes bytes while expecting {expect}", line!());
            break 'new_stream;
        }
        if sent < send_cnt || got < expect {
            // Don't print when we've reached the limit?
            // These tests could be structured differently ...
            println!("line {}: client has sent {sent} packets and got {got} bytes bytes while expecting {expect}", line!());
        }

        // TODO work on cancel of the `buffers` stream in-flight command. If we drop the stream
        // without it having completed the in-flight command an* we expect to be able to read
        // from the file descriptor again, we have to have the first in-flight cancelled first. In
        // fact, it seems data can be lost, and a bufring bid can be lost track of, if not
        // explicitly waiting for a cancel or close of the buffers stream. There is no outstanding
        // BufX created, but the bid will have sat in a CQE that didn't get mapped back to the
        // stream future to handle, so will have been disgarded. Another outstanding problem with
        // dropped streams is one or multiple more-links may be left in the slab. All these are
        // related to having the ability to cancel an in-flight operation where the op itself has
        // been dropped and the data it kept has been transferred to the slab's lifecycle entry.
    }
    Ok(())
}

async fn _client_ping_pong(
    rx: Rx,
    stream: TcpStream,
    send_cnt: usize,
    send_length: usize,
) -> io::Result<()> {
    // Implement client ping-pong loop. Make several read variations available.

    let mut send_buf = vec![1u8; send_length];
    let expect = send_buf.len();

    // Handle the recv_multi option separately because the stream created requires being pinned and
    // I couldn't figure out how to unpin it again to put it back into the Some(buffers) form.
    match &rx {
        Rx::RecvMulti(props) => {
            return _client_ping_pong_recv_multi(stream, send_cnt, send_buf, props).await
        }
        _ => {}
    };

    // Only used for two of the cases below, but logically, it's easier to create once even if it
    // won't be used.
    let recv_buf_length = match &rx {
        Rx::Read(props) => {
            let ReadProps { buf_size } = props;
            *buf_size
        }
        Rx::Recv(props) => {
            let RecvProps { buf_size, flags: _ } = props;
            *buf_size
        }
        _ => 0,
    };
    let mut recv_buf = vec![0u8; recv_buf_length];

    for _ in 0..send_cnt {
        let result;
        (result, send_buf) = stream.write_all(send_buf).await;
        let _ = result.unwrap();

        let mut got: usize = 0;

        while got < expect {
            match &rx {
                Rx::Read(_) => {
                    let result;
                    (result, recv_buf) = stream.read(recv_buf).await;
                    let read = result.unwrap();
                    if read == 0 {
                        return Err(Error::new(ErrorKind::Other, format!("client read returned 0, but got had reached only {}, while expected is {}", got, expect)));
                    }
                    got += read;
                }
                Rx::Recv(props) => {
                    let result;
                    (result, recv_buf) = stream.recv(recv_buf, props.flags).await;
                    let read = result.unwrap();
                    if read == 0 {
                        return Err(Error::new(ErrorKind::Other, format!("client read returned 0, but got had reached only {}, while expected is {}", got, expect)));
                    }
                    got += read;
                }
                Rx::RecvBufRing(props) => {
                    let BufRingProps {
                        buf_ring,
                        flags,
                        quiet_overflow,
                    } = props;
                    // Loop to handle case where the buf_ring pool was exhausted.
                    loop {
                        let bufx = stream.recv_provbuf(buf_ring.clone(), *flags).await;
                        match bufx {
                            Ok(Some(bufx)) => {
                                // If returning a Vec<u8> were necessary:
                                //  Either form of conversion from Bufx data to Vec<u8> could be appropriate here.
                                //  One consumes the BufX, the other doesn't and let's it drop here.
                                // break (Ok(bufx.len()), bufx.into())
                                // break (Ok(bufx.len()), bufx.as_slice().to_vec());
                                got += bufx.len();
                                break;
                            }
                            Ok(None) => {
                                // The connection is closed. But we are short of expected.
                                return Err(Error::new(ErrorKind::Other, format!("client read returned 0, but got had reached only {}, while expected is {}", got, expect)));
                            }
                            Err(e) => {
                                // Normal for some of the tests cases to cause the bufring to be exhausted.
                                if is_no_buffer_space_available(&e) {
                                    if !quiet_overflow {
                                        println!(
                                            "client: sleep 1us, recoverable recv_provbuf error {}",
                                            e
                                        );
                                    }
                                    time::sleep(time::Duration::from_micros(1)).await;
                                    continue;
                                }
                                return Err(
                                    Error::new(ErrorKind::Other,
                                    format!("client with got {got}, expect {expect}, sees recv_provbuf error {e}")));
                            }
                        }
                    }
                }
                Rx::RecvMulti(_) => {
                    // This case was handled earlier in the function.
                    unreachable!();
                }
            }
        }
    }
    Ok(())
}

async fn _server_ping_pong_reusing_vec(rx: Rx, stream: TcpStream, _local_addr: SocketAddr) {
    use tokio_uring::buf::BoundedBuf; // for slice()

    let recv_buf_length = match &rx {
        Rx::Read(props) => {
            let ReadProps { buf_size } = props;
            *buf_size
        }
        Rx::Recv(props) => {
            let RecvProps { buf_size, flags: _ } = props;
            *buf_size
        }
        _ => 0,
    };
    let mut buf = vec![0u8; recv_buf_length];
    let mut _n = 0;

    loop {
        let (result, nbuf) = match &rx {
            Rx::Read(_) => stream.read(buf).await,
            Rx::Recv(props) => stream.recv(buf, props.flags).await,
            Rx::RecvBufRing(_) => unreachable!(),
            Rx::RecvMulti(_) => unreachable!(),
        };
        buf = nbuf;
        let read = result.unwrap();
        if read == 0 {
            break;
        }

        let (res, slice) = stream.write_all(buf.slice(..read)).await;
        let _ = res.unwrap();
        buf = slice.into_inner();
        _n += read;
    }
}

async fn _server_ping_pong_using_recv_bufring_oneshot(
    stream: TcpStream,
    _local_addr: SocketAddr,
    props: &BufRingProps,
) {
    // Serve the connection by looping on input, each received bufx from the kernel which
    // we let go out of scope when we are done so it can be given back to the kernel.
    //
    // Here is a completion model based loop, as described in
    // https://github.com/axboe/liburing/wiki/io_uring-and-networking-in-2023
    // where the buffer being written to by the kernel is picked by the kernel from a
    // provided buffer pool, and when finished working with the buffer, it is returned to
    // the kernel's provided buffer pool.
    let BufRingProps {
        buf_ring,
        flags,
        quiet_overflow,
    } = props;

    let mut _n = 0;
    loop {
        // Loop to allow trying again if there was no buffer space available.
        let bufx = loop {
            let buf = stream.recv_provbuf(buf_ring.clone(), *flags).await;
            match buf {
                Ok(Some(buf)) => break buf,
                Ok(None) => {
                    // Normal that the client closed its connection and this
                    // server sees no more data is forthcoming. So the read
                    // amount was zero, so there was no buffer picked.
                    return;
                }
                Err(e) => {
                    // Expected error: No buffer space available (os error 105),
                    // for which we loop around.
                    //
                    // But sometimes getting error indicating the returned res was 0
                    // and flags was 4. Treat this like the connection is closed while
                    // awaiting confirmation from the io_uring team.
                    if e.kind() == std::io::ErrorKind::Other {
                        println!(
                            "server: assuming connection is closed: recv_provbuf error {}",
                            e
                        );
                        return;
                    }
                    // Normal for some of the tests cases to cause the bufring to be exhausted.
                    if !is_no_buffer_space_available(&e) {
                        panic!("server: recv_provbuf error {}", e);
                    }
                    if !quiet_overflow {
                        println!("server: sleep 1us, recoverable recv_provbuf error {}", e);
                    }
                    time::sleep(time::Duration::from_micros(1)).await;
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
        _n += read;
    }
}

async fn _server_ping_pong_using_recv_multishot(
    stream: TcpStream,
    _local_addr: SocketAddr,
    props: &BufRingProps,
) {
    // Serve the connection by looping on input, each received bufx from the kernel which
    // we let go out of scope when we are done so it can be given back to the kernel.
    //
    // Here is a completion model based loop, as described in
    // https://github.com/axboe/liburing/wiki/io_uring-and-networking-in-2023
    // where the buffer being written to by the kernel is picked by the kernel from a
    // provided buffer pool, and when finished working with the buffer, it is returned to
    // the kernel's provided buffer pool.
    let BufRingProps {
        buf_ring,
        flags,
        quiet_overflow,
    } = props;

    let mut total = 0;

    loop {
        let buffers = stream.recv_multi(buf_ring.clone(), *flags);
        tokio::pin!(buffers);
        while let Some(buf) = buffers.next().await {
            let bufx = match buf {
                Ok(buf) => buf,
                Err(e) => {
                    if is_no_buffer_space_available(&e) {
                        if !quiet_overflow {
                            println!("server: sleep 1us, recoverable recv_multi error {}", e);
                        }
                        time::sleep(time::Duration::from_micros(1)).await;
                        break;
                    }
                    if is_connection_reset_by_peer(&e) {
                        // This seems to be a normal condition that is sometimes caught, sometimes
                        // not. If the clients all end quickly, taking down their connection
                        // endpoints and the test can then complete, the fact that some or all of
                        // the server tasks see their connections reset by peer can go unreported.
                        println!("server: ending task after reading {total} bytes due to recv_multi error {}", e);
                        return;
                    }
                    panic!(
                        "server: after reading {total} bytes, fatal recv_multi error {}",
                        e
                    );
                }
            };

            let read = bufx.len();

            // The stream should not provided buffers with zero length. A zero length CQE result is
            // expected to mean the file descriptor will have no more data to read.
            assert!(read > 0);
            total += read;

            // Writing bufx or bufx.slice(..read) works equally well as the bufx length *is*
            // the length that was read.
            // let (res, _) = stream.write_all(bufx.slice(..read)).await;
            // let (res, _) = stream.write_all(bufx).await;
            //
            // The returned _ represents the BufX or slice of the BufX which we let go out of scope.

            let (res, _) = stream.write_all(bufx).await;

            match res {
                Ok(()) => {}
                Err(e) => {
                    if is_broken_pipe(&e) {
                        println!("server: ending task after reading {total} bytes due to sometimes normal write_all error {}", e);
                    } else {
                        println!("server: ending task after reading {total} bytes due to unexpected write_all error {}", e);
                    }
                    return;
                }
            }
        }
    }
}

pub async fn ping_pong_listener_loop(server: Server, listener: TcpListener) {
    let Server { rx, nodelay } = server;
    loop {
        let (stream, socket_addr) = listener.accept().await.unwrap();

        if nodelay {
            let _ = stream.set_nodelay(true).unwrap();
        }

        let rx = rx.clone();

        // Spawn task for each accepted connnection.
        tokio_uring::spawn(async move {
            match &rx {
                Rx::Read(_) | Rx::Recv(_) => {
                    _server_ping_pong_reusing_vec(rx, stream, socket_addr).await;
                }
                Rx::RecvBufRing(props) => {
                    _server_ping_pong_using_recv_bufring_oneshot(stream, socket_addr, props).await;
                }
                Rx::RecvMulti(props) => {
                    _server_ping_pong_using_recv_multishot(stream, socket_addr, props).await;
                }
            };
        });
    }
}

pub fn ping_pong_clients(
    clients: Clients,
    listener_addr: SocketAddr,
) -> oneshot::Receiver<io::Result<()>> {
    // Spawn clients as tokio_uring tasks and return a tokio oneshot receiver
    // that will indicate when they are done.

    let Clients {
        rx,
        nodelay,
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
            if nodelay {
                let _ = stream.set_nodelay(true).unwrap();
            }

            let res = _client_ping_pong(rx, stream, send_cnt, send_length).await;
            (client_id, res) // return through handle
        });
    }

    let (tx, rx) = oneshot::channel::<io::Result<()>>();

    tokio_uring::spawn(async move {
        let mut failed: usize = 0;
        let mut seen = vec![false; client_cnt];
        while let Some(res) = set.join_next().await {
            let (client_id, res) = res.unwrap();
            match res {
                Ok(()) => {
                    println!("client {client_id} succeeded");
                }
                Err(e) => {
                    failed += 1;
                    println!("client {client_id} failed: {e}");
                }
            }
            seen[client_id] = true;
        }

        for i in 0..client_cnt {
            assert!(seen[i]);
        }
        let res = if failed == 0 {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::Other,
                format!("{failed} client(s) failed"),
            ))
        };
        let _ = tx.send(res).unwrap();
    });

    rx
}

async fn _ping_pong(clients: Clients, server: Server) -> io::Result<()> {
    // Run `client_cnt` clients. Both clients and server use the TcpStream method identified by `rx`.

    let (listener, listener_addr) = tcp_listener().await.unwrap();

    // Spawn perhaps multiple clients

    let clients_done = ping_pong_clients(clients, listener_addr);

    // Spawn one listener, one server task will be spawned per connection accepted.

    tokio_uring::spawn(async move {
        ping_pong_listener_loop(server, listener).await;
    });

    // Wait until the clients signal they are done
    clients_done.await.unwrap()
}

pub struct Clients {
    pub rx: Rx,
    pub nodelay: bool,
    pub client_cnt: usize,
    pub send_cnt: usize,
    pub send_length: usize,
}

pub struct Server {
    pub rx: Rx,
    pub nodelay: bool,
}

pub struct PingPong {
    pub clients: Clients,
    pub server: Server,
}

impl PingPong {
    pub async fn run(self) -> io::Result<()> {
        let PingPong { clients, server } = self;
        _ping_pong(clients, server).await
    }
}
