use std::io::Write;

use tempfile::NamedTempFile;
use tokio_uring::fs::DmaFile;

const HELLO: &[u8] = b"hello world...";

fn tempfile() -> NamedTempFile {
    // we need to create the tempfile outside of /tmp, because /tmp may be a tmpfs and not support
    // direct io.
    NamedTempFile::new_in(".").unwrap()
}

#[test]
fn basic_read() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = DmaFile::open(tempfile.path()).await.unwrap();
        let buffer = file.alloc_dma_buffer(file.alignment());

        let (res, buf) = file.read_at(buffer, 0).await;

        let n = res.unwrap();

        assert_eq!(n, HELLO.len());
        assert_eq!(&buf[..n], HELLO);
    });
}

#[test]
fn basic_write() {
    tokio_uring::start(async {
        let tempfile = tempfile();
        let file = DmaFile::create(tempfile.path()).await.unwrap();
        let mut buffer = file.alloc_dma_buffer(dbg!(file.alignment()));
        buffer.extend_from_slice(&[0; 4096]);
        dbg!(buffer.len());
        buffer[..HELLO.len()].copy_from_slice(HELLO);
        let (res, buf) = file.write_at(buffer, 0).await;
        assert_eq!(res.unwrap(), 4096);

        let data = std::fs::read(tempfile.path()).unwrap();

        assert_eq!(&data[..], &buf[..]);
    });
}
