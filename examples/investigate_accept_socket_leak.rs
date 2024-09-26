// This example shows a socket, aka file descriptor, leak when an accept operation
// is dropped before it completed.
//
// The application issued the accept but then didn't await it for whatever reason.
//
// The output isn't pretty. It's doesn't actually count the FDs, it uses
// an 'ls' process to list the open FDs to stdout.

use std::process::Command;
use std::{cell::RefCell, io, rc::Rc};

use tokio::sync::oneshot;

use tokio_uring::net::{TcpListener, TcpStream};
use tokio_uring::spawn;

const ADDRESS: &str = "127.0.0.1:0";

fn main() -> io::Result<()> {
    tokio_uring::builder()
        .entries(16)
        .uring_builder(tokio_uring::uring_builder().setup_cqsize(32))
        .start(test_accept_fd_leak())
}

fn count_fd() {
    use std::process;
    // println!("My pid is {}", process::id());
    let id = process::id();
    let path = format!("/proc/{id}/fd/");

    let _output = Command::new("ls").arg("-l").arg(path).status();
}

async fn test_accept_fd_leak() -> io::Result<()> {
    let ln = TcpListener::bind(ADDRESS.parse().unwrap())?;
    let ln = Rc::new(RefCell::new(ln));

    let cnt = 10;

    /* Now that the leak is found, no need to run the no-leak version first.
    count_fd();
    println!("full {}", conn_with_full_accept(ln.clone(), cnt).await);
    count_fd();
    println!("full {}", conn_with_full_accept(ln.clone(), cnt).await);
    count_fd();
    println!("full {}", conn_with_dropped_accept(ln.clone(), cnt).await);
    count_fd();
    */

    let last0 = conn_with_dropped_accept(ln.clone(), cnt).await;
    println!("drop {last0}");

    let last1 = conn_with_dropped_accept(ln.clone(), cnt).await;
    println!("drop {last1}");

    let last2 = conn_with_dropped_accept(ln.clone(), cnt).await;
    println!("drop {last2}");

    //println!("full {}", conn_with_full_accept(ln.clone(), cnt).await);
    //println!("full {}", conn_with_full_accept(ln.clone(), cnt).await);

    if last0 == last2 {
        Ok(())
    } else if (last0 + 20) == last2 {
        // Expect this test to show 20 file descriptors were leaked as there
        // were two more calls to the test function, with 10 each.
        Err(tokio::io::Error::new(
            tokio::io::ErrorKind::Other,
            "As expected, each iteration leaked a file descriptor",
        ))
    } else {
        Err(tokio::io::Error::new(
            tokio::io::ErrorKind::Other,
            format!("file descriptors grew {}", last2 - last0),
        ))
    }
}

#[allow(dead_code)]
async fn conn_with_full_accept(ln: Rc<RefCell<TcpListener>>, connection_count: usize) -> i32 {
    let mut last_id = -1;

    for _ in 0..connection_count {
        let (tx_ch, rx_ch) = oneshot::channel();
        let addr = ln.borrow().local_addr().unwrap();

        let ln = ln.clone();
        let task = spawn(async move {
            let ln = ln.borrow();
            let future = ln.accept();

            _ = tx_ch.send(()).unwrap();

            let (stream, _) = future.await.unwrap();
            _ = stream.shutdown(std::net::Shutdown::Both);
        });

        let _ready = rx_ch.await.unwrap();

        let stream = TcpStream::connect(addr).await.unwrap();

        use std::os::unix::prelude::AsRawFd;
        last_id = stream.as_raw_fd();

        // Wait for task to complete *after* connection is established.
        _ = task.await;
    }

    last_id
}

async fn conn_with_dropped_accept(ln: Rc<RefCell<TcpListener>>, connection_count: usize) -> i32 {
    let mut last_id = -1;

    let mut connections: Vec<_> = Default::default();

    for _ in 0..connection_count {
        let addr = ln.borrow().local_addr().unwrap();
        let ln = ln.clone();

        let task = spawn(async move {
            let ln = ln.borrow();
            let future = ln.accept();

            // Create a way to get the first part of the future run before having it dropped.
            // This can then show the fd leak.
            tokio::select! {
                biased;
                _ = future => {
                    println!("future finished");
                }
                _ = async {} => {
                    println!("easy finished");
                }
            }
        });

        // let _ready = rx_ch.await.unwrap();

        // Wait for task to complete *before* establishing connection.
        // The future will have been cancelled but the uring accept operation will still be active.
        _ = task.await;

        let stream = TcpStream::connect(addr).await.unwrap();

        use std::os::unix::prelude::AsRawFd;
        last_id = stream.as_raw_fd();

        connections.push(stream);
    }

    drop(connections);
    println!("after dropping connection");
    count_fd(); // after the connections list is dropped
    last_id
}
