use tokio_uring::buf::IoBuf;
use tokio_uring::io;
use tokio_uring::fs::File;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let path = &args[1];

    tokio_uring::start(async {
        // Open the file without blocking
        let file = File::open(path).await.unwrap();

        let mut bufa = IoBuf::with_capacity(32 * 1024);
        let mut bufb = IoBuf::with_capacity(32 * 1024);

        // Fill the first buffer
        let n = file.read(bufa.tail()).await.unwrap();
        let mut done = n == 0;

        while !done {
            tokio::join!(
                async {
                    let n = io::stdout().write(bufa.slice(..)).await.unwrap();
                    assert_eq!(n, bufa.len());
                },
                async {
                    bufb.clear();
                    let n = file.read(bufb.tail()).await.unwrap();
                    done = n == 0;
                },
            );

            std::mem::swap(&mut bufa, &mut bufb);
        }
    });
}