use tempfile::NamedTempFile;

use tokio_uring::{buf::IoBuf, fs::File};

#[path = "../src/future.rs"]
#[allow(warnings)]
mod future;

#[test]
fn complete_ops_on_drop() {
    use std::sync::Arc;

    struct MyBuf {
        data: Vec<u8>,
        _ref_cnt: Arc<()>,
    }

    unsafe impl IoBuf for MyBuf {
        fn stable_ptr(&self) -> *const u8 {
            self.data.stable_ptr()
        }

        fn bytes_init(&self) -> usize {
            self.data.bytes_init()
        }

        fn bytes_total(&self) -> usize {
            self.data.bytes_total()
        }
    }

    unsafe impl tokio_uring::buf::IoBufMut for MyBuf {
        fn stable_mut_ptr(&mut self) -> *mut u8 {
            self.data.stable_mut_ptr()
        }

        unsafe fn set_init(&mut self, pos: usize) {
            self.data.set_init(pos);
        }
    }

    // Used to test if the buffer dropped.
    let ref_cnt = Arc::new(());

    let tempfile = tempfile();

    let vec = vec![0; 50 * 1024 * 1024];
    let mut file = std::fs::File::create(tempfile.path()).unwrap();
    std::io::Write::write_all(&mut file, &vec).unwrap();

    let file = tokio_uring::start(async {
        let file = File::create(tempfile.path()).await.unwrap();
        poll_once(async {
            file.read_at(
                MyBuf {
                    data: vec![0; 64 * 1024],
                    _ref_cnt: ref_cnt.clone(),
                },
                25 * 1024 * 1024,
            )
            .await
            .0
            .unwrap();
        })
        .await;

        file
    });

    assert_eq!(Arc::strong_count(&ref_cnt), 1);

    // little sleep
    std::thread::sleep(std::time::Duration::from_millis(100));

    drop(file);
}

#[test]
fn too_many_submissions() {
    let tempfile = tempfile();

    tokio_uring::start(async {
        let file = File::create(tempfile.path()).await.unwrap();
        for _ in 0..600 {
            poll_once(async {
                file.write_at(b"hello world".to_vec(), 0).await.0.unwrap();
            })
            .await;
        }
    });
}

#[test]
fn completion_overflow() {
    use std::process;
    use std::{thread, time};
    use tokio::task::JoinSet;

    let spawn_cnt = 50;
    let squeue_entries = 2;
    let cqueue_entries = 2 * squeue_entries;

    std::thread::spawn(|| {
        thread::sleep(time::Duration::from_secs(8)); // 1000 times longer than it takes on a slow machine
        eprintln!("Timeout reached. The uring completions are hung.");
        process::exit(1);
    });

    tokio_uring::builder()
        .entries(squeue_entries)
        .uring_builder(tokio_uring::uring_builder().setup_cqsize(cqueue_entries))
        .start(async move {
            let mut js = JoinSet::new();

            for _ in 0..spawn_cnt {
                js.spawn_local(tokio_uring::no_op());
            }

            while let Some(res) = js.join_next().await {
                res.unwrap().unwrap();
            }
        });
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

async fn poll_once(future: impl std::future::Future) {
    // use std::future::Future;
    use std::task::Poll;
    use tokio::pin;

    pin!(future);

    future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
