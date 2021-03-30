use tokio_uring::buf::IoBuf;
use tokio_uring::fs::File;
use std::ffi::OsString;
use std::rc::Rc;
use std::cell::Cell;

async fn count_file(path: OsString, count: Rc<Cell<u64>>) -> std::io::Result<()> {
    let file = File::open(path).await.unwrap();
    let mut buf = IoBuf::with_capacity(32 * 1024);
    loop {
        buf.clear();
        let n = file.read(buf.tail()).await?;
        if n == 0 {
            break;
        }

        let mut block_count = 0;
        for i in &*buf.slice(..) {
            if *i == b'a' {
                block_count += 1;
            }
        }
        count.set(count.get() + block_count);
    }

    Ok(())
}

fn main() {
    let files: Vec<_> = std::env::args_os().skip(1).collect();

    tokio_uring::start(async {
        let mut handles = Vec::with_capacity(files.len());

        let count = Rc::new(Cell::new(0));
        for file in files {
            let handle = tokio::task::spawn_local(count_file(file, count.clone()));
            handles.push(handle);
        }
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        println!("Total number of a's: {}", count.get());
    });
}
