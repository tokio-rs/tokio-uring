use tokio_uring::net::udp::UdpSocket;

const SOCK_1_ADDR: &str = "localhost:2401";
const SOCK_2_ADDR: &str = "localhost:4786";

const DATA_SENT: &str = "Hello world";

#[test]
fn smoke_test_udp() {
    tokio_uring::start(async {
        let sock1 = UdpSocket::bind(SOCK_1_ADDR).await.unwrap();
        let sock2 = UdpSocket::bind(SOCK_2_ADDR).await.unwrap();

        sock1.connect(SOCK_2_ADDR).await.unwrap();
        sock2.connect(SOCK_1_ADDR).await.unwrap();

        let sending_buf = DATA_SENT.as_bytes().to_owned();
        let receiving_buf = vec![0; sending_buf.len()];

        let (sending_res, sending_buf) = sock1.send(sending_buf).await;

        let bytes_sent = sending_res.unwrap();

        let (recv_res, receiving_buf) = sock2.recv(receiving_buf).await;

        let bytes_received = recv_res.unwrap();

        assert_eq!(bytes_sent, bytes_received, "Error, num bytes sent and received differ!");

        assert!(bytes_sent > 0, "Error, no bytes sent!");

        assert_eq!(
            &sending_buf[0..bytes_sent],
            &receiving_buf[0..bytes_received],
            "Error, buffers differ!"
        );
    });
}