use tokio_test::assert_err;
use tokio_uring::buf::fixed::BufRegistry;
use tokio_uring::buf::IoBuf;
use tokio_uring::fs::File;

use std::io::prelude::*;
use tempfile::NamedTempFile;

const HELLO: &[u8] = b"hello world...";

#[test]
fn unregister_invalidates_checked_out_buffers() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();

        let mut buffers = BufRegistry::new([Vec::with_capacity(1024)]);
        buffers.register().unwrap();

        let fixed_buf = buffers.check_out(0).unwrap();

        // The checked out handle keeps the buffer allocation alive.
        // Meanwhile, we replace buffer registration in the kernel:
        buffers.unregister().unwrap(); // hmm, what's up with the 1s blocking time?
        let mut buffers = BufRegistry::new([Vec::with_capacity(1024)]);
        buffers.register().unwrap();

        // The old buffer's index no longer matches the memory area of the
        // currently registered buffer, so the read operation using the old
        // buffer's memory should fail.
        let (res, _) = file.read_fixed_at(fixed_buf.slice(..), 0).await;
        assert_err!(res);

        let fixed_buf = buffers.check_out(0).unwrap();
        let (res, buf) = file.read_fixed_at(fixed_buf.slice(..), 0).await;
        let n = res.unwrap();
        assert_eq!(n, HELLO.len());
        assert_eq!(&buf[..], HELLO);
    });
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}
