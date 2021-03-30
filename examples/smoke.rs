use tokio_uring::fs::File;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let path = &args[1];

    tokio_uring::start(async {
        // Open the file without blocking
        let file = File::open(path).await.unwrap();

        for _ in 0..10 {
            // Fill the first buffer
            let buf = file.read_at2(0, 1024).await.unwrap();

            println!("BUF = {:?}", std::str::from_utf8(&buf[..]).unwrap().len());
        }
    });
}
