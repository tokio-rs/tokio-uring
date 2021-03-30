use tokio_uring::buf::IoBuf;
use tokio_uring::net::TcpListener;
use tokio_uring::runtime::Runtime;

use tokio::time::{self, Duration};

async fn cancel_accept() {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let listener = TcpListener::bind(addr).unwrap();

    tokio::select! {
        _ = listener.accept() => {
            panic!("wut, we accepted a socket??");
        }
        _ = time::sleep(Duration::from_millis(50)) => {
            println!("cancelled");
        }
    }
}

async fn cancel_read() {
    use tokio_uring::fs::File;
    use futures_lite::future::poll_fn;
    use std::future::Future;

    let f = File::open("/home/carllerche/Downloads/Win10_2004_English_x64.iso").await.unwrap();
    let mut buf = IoBuf::with_capacity(4096);

    let mut bufs = vec![];

    for i in 0..20 {
        bufs.push(IoBuf::with_capacity(4096));
    }

    println!("------------");

    for (i, buf) in bufs.iter_mut().enumerate() {
        let pos = (i + 100) * 1024 * 1024;
        // This schedules the op:
        let mut op = Box::pin(f.read_at(buf.tail(), pos as _));

        poll_fn(|cx| {
            let _ = op.as_mut().poll(cx);
            std::task::Poll::Ready(())
        }).await;

        drop(op);
    }
}

fn main() {
    let mut rt = Runtime::new().unwrap();

    rt.block_on(async {
        cancel_accept().await;
        // cancel_read().await;

        time::sleep(Duration::from_millis(50)).await;
    });
}