use tokio_test::assert_err;
use tokio_uring::buf::fixed::FixedBufRegistry;
use tokio_uring::buf::BoundedBuf;
use tokio_uring::fs::File;

use std::io::prelude::*;
use std::mem;
use tempfile::NamedTempFile;

const HELLO: &[u8] = b"hello world...";

#[test]
fn fixed_buf_turnaround() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();

        let buffers = FixedBufRegistry::new([30, 20, 10].iter().map(|&n| Vec::with_capacity(n)));
        buffers.register().unwrap();

        let fixed_buf = buffers.check_out(0).unwrap();
        assert_eq!(fixed_buf.bytes_total(), 30);

        // Can't check out the same buffer twice.
        assert!(buffers.check_out(0).is_none());

        // Checking out another buffer from the same registry is possible,
        // but does not affect the status of the first buffer.
        let fixed_buf1 = buffers.check_out(1).unwrap();
        assert_eq!(fixed_buf1.bytes_total(), 20);
        assert!(buffers.check_out(0).is_none());
        mem::drop(fixed_buf1);
        assert!(buffers.check_out(0).is_none());

        let op = file.read_fixed_at(fixed_buf, 0);

        // The buffer is used by the pending operation, can't check it out
        // for another instance.
        assert!(buffers.check_out(0).is_none());

        let (res, buf) = op.await;
        let n = res.unwrap();
        assert_eq!(n, HELLO.len());

        // The buffer is owned by `buf`, can't check it out
        // for another instance.
        assert!(buffers.check_out(0).is_none());

        mem::drop(buf);

        // The buffer has been released, check it out again.
        let fixed_buf = buffers.check_out(0).unwrap();
        assert_eq!(fixed_buf.bytes_total(), 30);
        assert_eq!(fixed_buf.bytes_init(), HELLO.len());
    });
}

#[test]
fn unregister_invalidates_checked_out_buffers() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();

        let buffers = FixedBufRegistry::new([Vec::with_capacity(1024)]);
        buffers.register().unwrap();

        let fixed_buf = buffers.check_out(0).unwrap();

        // The checked out handle keeps the buffer allocation alive.
        // Meanwhile, we replace buffer registration in the kernel:
        buffers.unregister().unwrap();
        let buffers = FixedBufRegistry::new([Vec::with_capacity(1024)]);
        buffers.register().unwrap();

        // The old buffer's index no longer matches the memory area of the
        // currently registered buffer, so the read operation using the old
        // buffer's memory should fail.
        let (res, _) = file.read_fixed_at(fixed_buf, 0).await;
        assert_err!(res);

        let fixed_buf = buffers.check_out(0).unwrap();
        let (res, buf) = file.read_fixed_at(fixed_buf, 0).await;
        let n = res.unwrap();
        assert_eq!(n, HELLO.len());
        assert_eq!(&buf[..], HELLO);
    });
}

#[test]
fn slicing() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::from_std(
            std::fs::File::options()
                .read(true)
                .write(true)
                .open(tempfile.path())
                .unwrap(),
        );

        let buffers = FixedBufRegistry::new([Vec::with_capacity(1024)]);
        buffers.register().unwrap();

        let fixed_buf = buffers.check_out(0).unwrap();

        let (res, slice) = file.read_fixed_at(fixed_buf.slice(..8), 3).await;
        let n = res.unwrap();
        assert_eq!(n, 8);
        assert_eq!(slice[..], HELLO[3..11]);
        let fixed_buf = slice.into_inner();

        let (res, slice) = file
            .write_fixed_at(fixed_buf.slice(1..), HELLO.len() as u64)
            .await;
        let n = res.unwrap();
        assert_eq!(n, 7);
        assert_eq!(slice[..], HELLO[4..11]);
        let fixed_buf = slice.into_inner();

        let (res, slice) = file.read_fixed_at(fixed_buf.slice(3..), 0).await;
        let n = res.unwrap();
        assert_eq!(n, HELLO.len() + 7);
        assert_eq!(slice[..HELLO.len()], HELLO[..]);
        assert_eq!(slice[HELLO.len()..], HELLO[4..11]);
    })
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}
