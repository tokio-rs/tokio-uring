use std::io;
use std::rc::Rc;
use tokio::task::JoinHandle;

pub const RESPONSE: &'static [u8] =
    b"HTTP/1.1 200 OK\nContent-Type: text/plain\nContent-Length: 12\n\nHello world!";

pub const ADDRESS: &'static str = "127.0.0.1:8080";

fn main() -> io::Result<()> {
    tokio_uring::start(async {
        let mut tasks = Vec::with_capacity(16);
        let listener = Rc::new(tokio_uring::net::TcpListener::bind(
            ADDRESS.parse().unwrap(),
        )?);

        for _ in 0..16 {
            let listener = listener.clone();
            let task: JoinHandle<io::Result<()>> = tokio::task::spawn_local(async move {
                loop {
                    let (stream, _) = listener.accept().await?;

                    tokio_uring::spawn(async move {
                        let (result, _) = stream.write(RESPONSE).await;

                        if let Err(err) = result {
                            eprintln!("Client connection failed: {}", err);
                        }
                    });
                }
            });
            tasks.push(task);
        }

        for t in tasks {
            t.await.unwrap()?;
        }

        Ok(())
    })
}
