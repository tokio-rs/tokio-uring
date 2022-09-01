use std::{
    io::prelude::*,
    os::unix::io::{AsRawFd, FromRawFd, RawFd},
};

use tempfile::NamedTempFile;

use tokio_uring::fs::File;

#[path = "../src/future.rs"]
#[allow(warnings)]
mod future;

const HELLO: &[u8] = b"hello world...";

async fn read_hello(file: &File) {
    let buf = Vec::with_capacity(1024);
    let (res, buf) = file.read_at(buf, 0).await;
    let n = res.unwrap();

    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[test]
fn basic_read() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();
        read_hello(&file).await;
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
fn vectored_read() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();
        let bufs = vec![Vec::<u8>::with_capacity(5), Vec::<u8>::with_capacity(9)];
        let (res, bufs) = file.readv_at(bufs, 0).await;
        let n = res.unwrap();

        assert_eq!(n, HELLO.len());
        assert_eq!(bufs[1][0], b' ');
    });
}

#[test]
fn vectored_write() {
    tokio_uring::start(async {
        let tempfile = tempfile();

        let file = File::create(tempfile.path()).await.unwrap();
        let buf1 = "hello".to_owned().into_bytes();
        let buf2 = " world...".to_owned().into_bytes();
        let bufs = vec![buf1, buf2];

        file.writev_at(bufs, 0).await.0.unwrap();

        let file = std::fs::read(tempfile.path()).unwrap();
        assert_eq!(file, HELLO);
    });
}

#[test]
fn cancel_read() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let file = File::open(tempfile.path()).await.unwrap();

        // Poll the future once, then cancel it
        poll_once(async { read_hello(&file).await }).await;

        read_hello(&file).await;
    });
}

#[test]
fn explicit_close() {
    let mut tempfile = tempfile();
    tempfile.write_all(HELLO).unwrap();

    tokio_uring::start(async {
        let file = File::open(tempfile.path()).await.unwrap();
        let fd = file.as_raw_fd();

        file.close().await.unwrap();

        assert_invalid_fd(fd);
    })
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

#[test]
fn drop_off_runtime() {
    let file = tokio_uring::start(async {
        let tempfile = tempfile();
        File::open(tempfile.path()).await.unwrap()
    });

    let fd = file.as_raw_fd();
    drop(file);

    assert_invalid_fd(fd);
}

#[test]
fn sync_doesnt_kill_anything() {
    let tempfile = tempfile();

    tokio_uring::start(async {
        let file = File::create(tempfile.path()).await.unwrap();
        file.sync_all().await.unwrap();
        file.sync_data().await.unwrap();
        file.write_at(&b"foo"[..], 0).await.0.unwrap();
        file.sync_all().await.unwrap();
        file.sync_data().await.unwrap();
    });
}

#[test]
fn rename() {
    use std::ffi::OsStr;
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        tempfile.write_all(HELLO).unwrap();

        let old_path = tempfile.path();
        let old_file = File::open(old_path).await.unwrap();
        read_hello(&old_file).await;
        old_file.close().await.unwrap();

        let mut new_file_name = old_path
            .file_name()
            .unwrap_or_else(|| OsStr::new(""))
            .to_os_string();
        new_file_name.push("_renamed");

        let new_path = old_path.with_file_name(new_file_name);

        tokio_uring::fs::rename(&old_path, &new_path).await.unwrap();

        let new_file = File::open(&new_path).await.unwrap();
        read_hello(&new_file).await;

        let old_file = File::open(old_path).await;
        assert!(old_file.is_err());

        // Since the file has been renamed, it won't be deleted
        // in the TempPath destructor. We have to manually delete it.
        std::fs::remove_file(&new_path).unwrap();
    })
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

async fn poll_once(future: impl std::future::Future) {
    use future::poll_fn;
    // use std::future::Future;
    use std::task::Poll;
    use tokio::pin;

    pin!(future);

    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

fn assert_invalid_fd(fd: RawFd) {
    use std::fs::File;

    let mut f = unsafe { File::from_raw_fd(fd) };
    let mut buf = vec![];

    match f.read_to_end(&mut buf) {
        Err(ref e) if e.raw_os_error() == Some(libc::EBADF) => {}
        res => panic!("{:?}", res),
    }
}
