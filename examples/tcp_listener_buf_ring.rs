// Test the buf_ring aspect of the TcpStream read.
//
// Initially just build up the server, later add a client and run the test between them.
// For now, tests have used scripts running many nc in the background to create hundreds
// of background processes, each sending 100 messages, one per second.
//
// The accept loop will run indefinitely until the process is aborted.
//
// Run with -h to see the usage displayed.

use std::io;
use std::net::SocketAddr;

use tokio_uring::bufgroup::Bgid;
use tokio_uring::bufring;
use tokio_uring::bufring::BufRingRc;
use tokio_uring::net::TcpListener;
use tokio_uring::net::TcpStream;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "Serve TCP connections, testing the buf_ring feature. Client can be 'nc <ip-addr> <port>'.", long_about = None)]
struct Args {
    /// Listen address
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    socket_addr: String,

    /// Group ID
    #[arg(long, default_value_t = 999)]
    bgid: Bgid,

    /// Ring entries
    #[arg(long, default_value_t = 128)]
    ring_entries: u16,

    /// Buffer count
    #[arg(long, default_value_t = 0)]
    buf_cnt: u16,

    /// Buffer length
    #[arg(long, default_value_t = 4096)]
    buf_len: usize,

    /// Buffer alignment
    #[arg(long, default_value_t = 0)]
    buf_align: usize,

    /// Minimum pad to place between end of ring and first buffer
    #[arg(long, default_value_t = 0)]
    ring_pad: usize,

    /// Buffer-end alignment
    #[arg(long, default_value_t = 0)]
    bufend_align: usize,

    /// Release delay, delays the release of each buf_ring buffer if non-zero
    #[arg(long, default_value = "0")]
    #[arg(value_parser = parse_duration_ms)]
    release_delay_ms: std::time::Duration,
}

fn parse_duration_ms(arg: &str) -> Result<std::time::Duration, std::num::ParseIntError> {
    let ms = arg.parse()?;
    Ok(std::time::Duration::from_millis(ms))
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let socket_addr = args.socket_addr;
    let bgid = args.bgid;
    let ring_entries = args.ring_entries;
    let buf_cnt = args.buf_cnt;
    let buf_len = args.buf_len;
    let buf_align = args.buf_align;
    let ring_pad = args.ring_pad;
    let bufend_align = args.bufend_align;
    let release_delay_ms = args.release_delay_ms;

    let socket_addr: SocketAddr = socket_addr.parse().unwrap();

    tokio_uring::start(async {
        let buf_ring = bufring::Builder::new(bgid)
            .ring_entries(ring_entries)
            .buf_cnt(buf_cnt)
            .buf_len(buf_len)
            .buf_align(buf_align)
            .ring_pad(ring_pad)
            .bufend_align(bufend_align)
            .build()?;

        // // On older kernels, will fail with Invalid argument (os error 22)
        // buf_ring.borrow().register()?;
        //
        // // Check that duplicate is caught with appropriate error.
        // buf_ring.register()?;

        let listener = TcpListener::bind(socket_addr).unwrap();

        println!("Listening on {}", listener.local_addr().unwrap());

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let buf_ring = buf_ring.clone();

            tokio_uring::spawn(async move {
                if let Err(e) = pingpong(buf_ring, stream, peer_addr, release_delay_ms).await {
                    eprintln!("connection error: {e}")
                }
            });
        }
        // TODO fix this allow
        #[allow(unreachable_code)]
        Ok::<(), std::io::Error>(())
    })?;
    Ok(())
}

async fn pingpong(
    buf_ring: BufRingRc,
    stream: TcpStream,
    peer_addr: SocketAddr,
    release_delay_ms: std::time::Duration,
) -> io::Result<()> {
    // Implement ping-pong loop.
    // Pass the buffer provided from the buffer ring by the read_bg directly to the write.

    println!("{} connected", peer_addr);
    let mut bytes = 0;
    //let mut pkts = 0;

    loop {
        let bufx = match stream.read_bg(buf_ring.clone()).await {
            Err(e) => {
                if let Some(raw_os_err) = e.raw_os_error() {
                    if raw_os_err == 105 {
                        eprintln!("ring buffers exhausted, sleeping 1 second");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                }
                eprintln!("accept_multishot accept_stream.next returned {}", e);
                return Err(e);
            }
            Ok(res) => res,
        };

        let read = bufx.len();
        if read == 0 {
            println!(
                "{} closed, {} total bytes ping-ponged, buf_ring possible_min {}\n\n",
                peer_addr,
                bytes,
                buf_ring.possible_min()
            );
            break;
        }
        bytes += read;
        //pkts += 1;

        if !release_delay_ms.is_zero() {
            tokio::time::sleep(release_delay_ms).await;
        }

        // We can pass bufx directly because BufX implements the IoBuf trait.

        let (res, _) = stream.write_all(bufx).await;
        res.unwrap();
        println!(
            "{} all {} bytes ping-ponged, running total {} bytes",
            peer_addr, read, bytes
        );
    }
    Ok(())
}
