use tokio::net::{TcpListener, TcpStream};

#[test]
fn use_tokio_types_from_runtime() {
    tokio_uring::start(async {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let task = tokio::spawn(async move {
            let _socket = TcpStream::connect(addr).await.unwrap();
        });

        // Accept a connection
        let (_socket, _) = listener.accept().await.unwrap();

        // Wait for the task to complete
        task.await.unwrap();
    });
}
