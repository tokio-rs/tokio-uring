use std::sync::mpsc::sync_channel;
use std::thread;
use tokio::io;
use tokio_stream::StreamExt;
use tokio_uring::buf::bufring;
use tokio_uring::net::TcpStream;

mod common;

use common::probe;
use common::Rx;

#[test]
fn net_tcp_ping_pong_read_one() -> io::Result<()> {
    // Run one client. Both client and server use the TcpStream `read` method.

    tokio_uring::start(async {
        common::PingPong {
            clients: common::Clients {
                rx: Rx::Read(common::ReadProps {
                    buf_size: 16 * 1024,
                }),
                nodelay: false,
                client_cnt: 1,
                send_cnt: 10,
                send_length: 1024,
            },
            server: common::Server {
                rx: Rx::Read(common::ReadProps {
                    buf_size: 16 * 1024,
                }),
                nodelay: false,
            },
        }
        .run()
        .await
    })
}

#[test]
fn net_tcp_ping_pong_read_several() -> io::Result<()> {
    // Run 3 clients. Both clients and server use the TcpStream `read` method.

    tokio_uring::start(async {
        common::PingPong {
            clients: common::Clients {
                rx: Rx::Read(common::ReadProps {
                    buf_size: 16 * 1024,
                }),
                nodelay: false,
                client_cnt: 3,
                send_cnt: 10,
                send_length: 1024,
            },
            server: common::Server {
                rx: Rx::Read(common::ReadProps {
                    buf_size: 16 * 1024,
                }),
                nodelay: false,
            },
        }
        .run()
        .await
    })
}

#[test]
fn net_tcp_ping_pong_recv() -> io::Result<()> {
    // Run 3 clients. Both clients and server use the TcpStream `recv` method.

    tokio_uring::start(async {
        common::PingPong {
            clients: common::Clients {
                rx: Rx::Recv(common::RecvProps {
                    buf_size: 16 * 1024,
                    flags: None,
                }),
                nodelay: false,
                client_cnt: 3,
                send_cnt: 10,
                send_length: 1024,
            },
            server: common::Server {
                rx: Rx::Recv(common::RecvProps {
                    buf_size: 16 * 1024,
                    flags: None,
                }),
                nodelay: false,
            },
        }
        .run()
        .await
    })
}

#[test]
fn net_tcp_ping_pong_recv_oneshot_bufring_1_thread() -> io::Result<()> {
    if !probe::is_buf_ring_supported() {
        eprintln!("skipping test, buf_ring is not supported in this kernel");
        return Ok(());
    }
    // Run 5 clients. Both clients and server use the TcpStream `recv` method with a BufRing pool
    // that is built small enough (4 entries) that there will be some pool exhaustion that has to
    // be handled by retrying the requests.
    // And a bit oddly, both clients and server are using the same BufRing, as they are all run in
    // the same tokio_uring instance.

    tokio_uring::start(async {
        let buf_ring = bufring::Builder::new(177)
            .ring_entries(4)
            .buf_len(4096)
            // Normally, no reason to skip the auto register, but this let's us test the manual
            // register below.
            .skip_auto_register(true)
            .build()
            .unwrap();

        buf_ring.register().unwrap();

        common::PingPong {
            clients: common::Clients {
                rx: Rx::RecvBufRing(common::BufRingProps {
                    buf_ring: buf_ring.clone(),
                    flags: None,
                    quiet_overflow: true, // normal for this test to cause pool exhaustion
                }),
                nodelay: false,
                client_cnt: 10,
                send_cnt: 10,
                send_length: 1024,
            },
            server: common::Server {
                rx: Rx::RecvBufRing(common::BufRingProps {
                    buf_ring: buf_ring.clone(),
                    flags: None,
                    quiet_overflow: true, // normal for this test to cause pool exhaustion
                }),
                nodelay: false,
            },
        }
        .run()
        .await?;

        // Manually unregistering the buf_ring. When it goes out of scope, it is unregistered
        // automatically. Note, it remains in scope while there are outstanding buffers the
        // application hasn't dropped yet.
        buf_ring.unregister().unwrap();
        Ok(())
    })
}

#[test]
fn net_tcp_ping_pong_recv_multishot_bufring_1_thread() -> io::Result<()> {
    if !probe::is_buf_ring_supported() {
        eprintln!("skipping test, buf_ring is not supported in this kernel");
        return Ok(());
    }
    // Run 5 clients. Both clients and server use the TcpStream `recv` method with a BufRing pool
    // that is built small enough (4 entries) that there will be some pool exhaustion that has to
    // be handled by retrying the requests.
    // And a bit oddly, both clients and server are using the same BufRing, as they are all run in
    // the same tokio_uring instance.

    tokio_uring::start(async {
        let client_buf_ring = bufring::Builder::new(177)
            .ring_entries(4)
            .buf_len(4096)
            // Normally, no reason to skip the auto register, but this let's us test the manual
            // register below.
            .skip_auto_register(true)
            .build()
            .unwrap();
        let server_buf_ring = bufring::Builder::new(178)
            .ring_entries(2)
            .buf_len(4096)
            // Normally, no reason to skip the auto register, but this let's us test the manual
            // register below.
            .skip_auto_register(true)
            .build()
            .unwrap();

        client_buf_ring.register().unwrap();
        server_buf_ring.register().unwrap();

        common::PingPong {
            clients: common::Clients {
                rx: Rx::RecvMulti(common::BufRingProps {
                    buf_ring: client_buf_ring.clone(),
                    flags: None,
                    quiet_overflow: false,
                }),
                nodelay: false,
                client_cnt: 4,
                send_cnt: 10,
                send_length: 1024,
            },
            server: common::Server {
                rx: Rx::RecvMulti(common::BufRingProps {
                    buf_ring: server_buf_ring.clone(),
                    flags: None,
                    quiet_overflow: true, // normal for this test to have to retry given the small pool size
                }),
                nodelay: false,
            },
        }
        .run()
        .await?;

        // Manually unregistering the buf_rings. When any goes out of scope, it is unregistered
        // automatically. Note, it remains in scope while there are outstanding buffers the
        // application hasn't dropped yet.
        client_buf_ring.unregister().unwrap();
        server_buf_ring.unregister().unwrap();
        Ok(())
    })
}

#[test]
fn net_tcp_ping_pong_recv_oneshot_bufring_2_threads() -> io::Result<()> {
    if !probe::is_buf_ring_supported() {
        eprintln!("skipping test, buf_ring is not supported in this kernel");
        return Ok(());
    }
    // Similar to test net_tcp_ping_pong_recv_bufring above, but uses two new threads,
    // one for the server code, one for all the clients.
    //
    // Two std thread syncing methods employed: a sync_channel gets the ephemeral port from one
    // thread back to the main thread, and the main thread then is blocked at the end, waiting for
    // the clients thread handle to report the clients thread is done.
    //
    // There is no attempt to shutdown the server thread.
    //
    //
    // Further details:
    //
    //  The server thread starts a tokio_uring runtime, creates a provided buffers buf_ring,
    //  and listens on a port, spawning tasks to serve the connections being established. All
    //  server task share the same provided buffer pool buf_ring.
    //
    //  The client thread also starts a tokio_uring runtime, also creates a provided buffers
    //  buf_ring, and spawns as many client tasks as the constant below dictates. Each client task
    //  uses its own Vec<u8> buffer for writing data but all share the same buf_ring for receiving
    //  data back from its stream.
    //
    // Minutia:
    //
    //  The buffer group id, bgid, assigned to each buf_ring, one for the server, one for the
    //  clients, are in independant spaces, so could have the same value. They are chosen here as
    //  261 and 262, respectively, but they could both be 261. They could both be zero for that
    //  matter.

    use libc::{sysconf, _SC_PAGESIZE};
    let page_size: usize = unsafe { sysconf(_SC_PAGESIZE) as usize };

    /*
     * These yield a test run that takes about 2.8s
    const CLIENT_CNT: usize = 32;
    const SENDS_PER_CLIENT: usize = 64;
    const SEND_LENGTH: usize = 64 * 1024;
    const CLIENT_BUFRING_SIZE: u16 = 64;
    const SERVER_BUFRING_SIZE: u16 = 64;
     */
    const CLIENT_CNT: usize = 4;
    const SENDS_PER_CLIENT: usize = 4;
    const SEND_LENGTH: usize = 4 * 1024;
    const CLIENT_BUFRING_SIZE: u16 = 8;
    const SERVER_BUFRING_SIZE: u16 = 8;
    const CLIENT_BUF_LEN: usize = 4096;
    const SERVER_BUF_LEN: usize = 4096;

    // Used by the thread running the server to pass its ephemeral local port to the thread
    let (addr_tx, addr_rx) = sync_channel::<std::net::SocketAddr>(0);

    let _server_handle = thread::spawn(move || {
        tokio_uring::start(async {
            let buf_ring = bufring::Builder::new(261)
                .page_size(page_size)
                .ring_entries(SERVER_BUFRING_SIZE)
                .buf_len(SERVER_BUF_LEN)
                .build()
                .unwrap();
            let server = common::Server {
                rx: Rx::RecvBufRing(common::BufRingProps {
                    buf_ring: buf_ring.clone(),
                    flags: None,
                    quiet_overflow: false,
                }),
                nodelay: false,
            };
            let (listener, local_addr) = common::tcp_listener().await.unwrap();
            addr_tx.send(local_addr).unwrap();

            common::ping_pong_listener_loop(server, listener).await;
        });
    });

    let listener_addr = addr_rx.recv().unwrap();

    let clients_handle = thread::spawn(move || {
        tokio_uring::start(async {
            let buf_ring = bufring::Builder::new(262)
                .page_size(page_size)
                .ring_entries(CLIENT_BUFRING_SIZE as u16)
                .buf_len(CLIENT_BUF_LEN)
                .build()
                .unwrap();
            let clients = common::Clients {
                rx: Rx::RecvBufRing(common::BufRingProps {
                    buf_ring: buf_ring.clone(),
                    flags: None,
                    quiet_overflow: false,
                }),
                nodelay: false,
                client_cnt: CLIENT_CNT,
                send_cnt: SENDS_PER_CLIENT,
                send_length: SEND_LENGTH,
            };
            let clients_done = common::ping_pong_clients(clients, listener_addr);

            // Wait for the clients tasks to be done.

            clients_done.await.unwrap()
        })
    });

    // Wait for the clients thread to finish.
    clients_handle.join().unwrap()
}

#[test]
fn net_tcp_ping_pong_recv_multishot_bufring_2_threads() -> io::Result<()> {
    if !probe::is_buf_ring_supported() {
        eprintln!("skipping test, buf_ring is not supported in this kernel");
        return Ok(());
    }

    use libc::{sysconf, _SC_PAGESIZE};
    let page_size: usize = unsafe { sysconf(_SC_PAGESIZE) as usize };

    /*
     * Even with SERVER_NODELAY = true,
     * this yields a test run that takes about 36s/debug, 20s/release.
     * This one is very interesting because it hits the case where the multishot recv was completed
     * while there was still more data to read. So the client logic had to be rewritten to allow
     * the multishot recv commands to be issued more than once while remembering how far the client
     * had gotten in sending packets and in receiving the parts of the packet.
    const CLIENT_CNT: usize = 32;
    const SENDS_PER_CLIENT: usize = 64 * 64;
    const SEND_LENGTH: usize = 64 * 1024;
    const CLIENT_BUFRING_SIZE: u16 = 16 * 64;
    const SERVER_BUFRING_SIZE: u16 = 16 * 64;
    const CLIENT_NODELAY: bool = true;
    const SERVER_NODELAY: bool = true;
     */
    /*
     * These yields a test run that takes about 3s,
     * unless the SERVER_NODELAY is set to true, and then only .7s.
    const CLIENT_CNT: usize = 32;
    const SENDS_PER_CLIENT: usize = 64;
    const SEND_LENGTH: usize = 64 * 1024;
    const CLIENT_BUFRING_SIZE: u16 = 16*64;
    const SERVER_BUFRING_SIZE: u16 = 16*64;
    const CLIENT_NODELAY: bool = false;
    const SERVER_NODELAY: bool = true;
     */
    /*
     * The normal small test case.
     */
    const CLIENT_CNT: usize = 4;
    const SENDS_PER_CLIENT: usize = 4;
    const SEND_LENGTH: usize = 4 * 1024;
    const CLIENT_BUFRING_SIZE: u16 = 8;
    const SERVER_BUFRING_SIZE: u16 = 8;
    const CLIENT_NODELAY: bool = false;
    const SERVER_NODELAY: bool = false;

    const CLIENT_BUF_LEN: usize = 4096;
    const SERVER_BUF_LEN: usize = 4096;

    // Used by the thread running the server to pass its ephemeral local port to the thread
    let (addr_tx, addr_rx) = sync_channel::<std::net::SocketAddr>(0);

    let _server_handle = thread::spawn(move || {
        tokio_uring::start(async {
            let buf_ring = bufring::Builder::new(261)
                .page_size(page_size)
                .ring_entries(SERVER_BUFRING_SIZE)
                .buf_len(SERVER_BUF_LEN)
                .build()
                .unwrap();
            let server = common::Server {
                rx: Rx::RecvMulti(common::BufRingProps {
                    buf_ring: buf_ring.clone(),
                    flags: None,
                    quiet_overflow: false,
                }),
                nodelay: SERVER_NODELAY,
            };
            let (listener, local_addr) = common::tcp_listener().await.unwrap();
            addr_tx.send(local_addr).unwrap();

            common::ping_pong_listener_loop(server, listener).await;
        });
    });

    let listener_addr = addr_rx.recv().unwrap();

    let clients_handle = thread::spawn(move || {
        tokio_uring::start(async {
            let buf_ring = bufring::Builder::new(262)
                .page_size(page_size)
                .ring_entries(CLIENT_BUFRING_SIZE as u16)
                .buf_len(CLIENT_BUF_LEN)
                .build()
                .unwrap();
            let clients = common::Clients {
                rx: Rx::RecvMulti(common::BufRingProps {
                    buf_ring: buf_ring.clone(),
                    flags: None,
                    quiet_overflow: false,
                }),
                nodelay: CLIENT_NODELAY,
                client_cnt: CLIENT_CNT,
                send_cnt: SENDS_PER_CLIENT,
                send_length: SEND_LENGTH,
            };
            let clients_done = common::ping_pong_clients(clients, listener_addr);

            // Wait for the clients tasks to be done.

            clients_done.await.unwrap()
        })
    });

    // Wait for the clients thread to finish.
    clients_handle.join().unwrap()
}

#[test]
fn net_accept_multi_1_thread() -> io::Result<()> {
    const CONNECTIONS: usize = 4;

    println!("Test to get {CONNECTIONS} TCP connections accepted");

    tokio_uring::start(async {
        let (listener, listener_addr) = common::tcp_listener().await.unwrap();

        let mut clients: Vec<_> = vec![];

        for _ in 0..CONNECTIONS {
            let stream = TcpStream::connect(listener_addr).await;
            let stream = match stream {
                Ok(stream) => stream,
                Err(e) => {
                    if common::is_too_many_open_files(&e) {
                        println!("expected: {}", e);
                    } else {
                        println!("unexpected: {}", e);
                    }
                    break;
                }
            };
            clients.push(stream);
        }

        let streams = listener.accept_multi(None);
        tokio::pin!(streams);

        let mut connected_count: usize = 0;
        while let Some(_stream) = streams.next().await {
            connected_count += 1;
            println!("another stream connected, number {connected_count}");

            if connected_count == clients.len() {
                println!("breaking out of loop");
                break;
            }
        }
        println!("loop done");
    });
    println!("tokio_ring::start done");

    Ok(())
}
