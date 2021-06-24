use tokio_uring::fs::File;

use std::io::prelude::*;
use tempfile::NamedTempFile;

const HELLO: &[u8] = b"hello world...";

#[test]
fn basic_read() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();

        let buf = Vec::with_capacity(1024);
        let (res, buf) = file.read_at(buf, 0).await;
        let n = res.unwrap();

        assert_eq!(n, HELLO.len());
        assert_eq!(&buf[..n], HELLO);
    });
}

#[test]
fn basic_write() {
    tokio_uring::start(async {
        let tempfile = tempfile();

        let file = File::create(tempfile.path()).await.unwrap();

        file.write_at(HELLO, 0).await.0.unwrap();

        let file = std::fs::read(tempfile.path()).unwrap();
        assert_eq!(file, HELLO);
    });
}

#[test]
fn drop_open() {
    tokio_uring::start(async {
        let tempfile = tempfile();
        let _ = File::create(tempfile.path());

        // Do something else
        let file = File::create(tempfile.path()).await.unwrap();

        file.write_at(HELLO, 0).await.0.unwrap();

        let file = std::fs::read(tempfile.path()).unwrap();
        assert_eq!(file, HELLO);
    });
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}
