use tokio_uring::buf::IoBufMut;
use tokio_uring::fs::File;

use tempfile::NamedTempFile;
use std::io::prelude::*;

const HELLO: &[u8] = b"hello world...";

#[test]
fn basic_read() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();
    
        let file = File::open(tempfile.path()).await.unwrap();
    
        let buf = IoBufMut::with_capacity(1024);
        let (res, buf) = file.read_at(buf.slice(..), 0).await;
        let n = res.unwrap();
    
        assert_eq!(n, HELLO.len());
        assert_eq!(&buf[..n], HELLO);

    });
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}