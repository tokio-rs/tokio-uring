use crate::buf::{self, IoBuf, IoBufMut};
use crate::driver::{self, Op};

use futures::ready;
use tokio::io::ReadBuf;
use std::cell::RefCell;
use std::io;
use std::mem;
use std::os::unix::io::RawFd;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Sequential read / write streams.
///
/// Backs File and TcpStream.
pub(crate) struct Stream {
    /// Stream file descriptor: File or TcpStream.
    fd: RawFd,

    /// State of read/write operations
    state: RefCell<State>,
}

struct State {
    /// Buffers data received from read operations
    read_buf: IoBufMut,

    /// Position read to
    read_pos: usize,

    /// Tracks read operation state
    read: Read,

    /// Buffers data received for write operations
    write_buf: buf::SliceMut,

    /// Tracks write operation state.
    write: Write,
}

// Streaming read operations.
enum Read {
    Idle,
    Reading(Op<driver::Read>),
    Shutdown,
}

/// Streaming write operations
enum Write {
    Idle,
    Writing(Op<driver::Write>),
}

const DEFAULT_BUF_SIZE: usize = 4 * 1024;

impl Stream {
    pub(crate) fn new(fd: RawFd) -> Stream {
        Stream {
            fd,
            state: RefCell::new(State {
                read_buf: IoBufMut::with_capacity(DEFAULT_BUF_SIZE),
                read_pos: 0,
                read: Read::Idle,
                write_buf: IoBufMut::with_capacity(DEFAULT_BUF_SIZE).slice(..),
                write: Write::Idle,
            })
        }
    }

    pub(crate) fn poll_read(&self, cx: &mut Context<'_>, dst: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        use std::cmp;

        let mut state = self.state.borrow_mut();

        let src = ready!(state.poll_fill_buf(cx, self.fd))?;
        let n = cmp::min(src.len(), dst.remaining());

        dst.put_slice(&src[..n]);
        state.consume(n);

        Poll::Ready(Ok(()))
    }

    pub(crate) fn poll_fill_buf(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let fd = self.fd;
        RefCell::get_mut(&mut self.state).poll_fill_buf(cx, fd)
    }

    pub(crate) fn consume(&mut self, amt: usize) {
        RefCell::get_mut(&mut self.state).consume(amt)
    }

    pub(crate) fn poll_write(&self, cx: &mut Context<'_>, src: &[u8]) -> Poll<io::Result<usize>> {
        use std::cmp;

        let mut state = self.state.borrow_mut();
        let state = &mut *state;

        let n = {
            let dst = ready!(state.poll_sink_buf(cx, self.fd))?;
            let n = cmp::min(src.len(), dst.capacity());

            // TODO: extract
            unsafe {
                dst.as_mut_ptr().copy_from_nonoverlapping(src.as_ptr(), src.len());
                let new_len = n + dst.begin();
                dst.get_inner_mut().set_len(new_len);
            }

            n
        };

        state.produce(n);
        state.flush_if_full(self.fd)?;

        Poll::Ready(Ok(n))
    }

    pub(crate) fn poll_flush(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.state.borrow_mut().poll_flush(cx, self.fd)
    }

    pub(crate) fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl State {
    /// Fill the internal buffer, returning a reference
    fn poll_fill_buf(&mut self, cx: &mut Context<'_>, fd: RawFd) -> Poll<io::Result<&[u8]>> {
        loop {
            // If there is buffered data, return it
            // TODO: ensure an operation is in-flight.
            match &mut self.read {
                Read::Idle => {
                    if !self.read_buf[self.read_pos..].is_empty() {
                        return Poll::Ready(Ok(&self.read_buf[self.read_pos..]));
                    }

                    // Start a new in-flight operation
                    self.read_buf.clear();
                    self.read_pos = 0;

                    // Take the buffer out of `self` so it can be submitted to the kernel.
                    let buf = mem::replace(&mut self.read_buf, IoBufMut::with_capacity(0));
                    let len = buf.len();

                    // Read into the end of the buffer
                    let op = Op::read_at(fd, buf.slice(len..), 0)?;

                    self.read = Read::Reading(op);
                }
                Read::Reading(op) => {
                    // Complete the read and get the buffer back
                    let (res, slice) = ready!(Pin::new(op).poll_read(cx));

                    println!(" + READ DONE; {:?}", res);

                    // Place the buffer back
                    assert_eq!(self.read_buf.capacity(), 0);
                    self.read_buf = slice.into_inner();
            
                    let n = res?;
            
                    unsafe { self.read_buf.set_len(self.read_buf.len() + n); }

                    if n == 0 {
                        self.read = Read::Shutdown;
                    } else {
                        self.read = Read::Idle;
                    }
                }
                Read::Shutdown => {
                    return Poll::Ready(Ok(&self.read_buf[self.read_pos..]));
                }
            }
        }
    }

    fn consume(&mut self, amt: usize) {
        assert!(self.read_buf.len() <= self.read_pos + amt);
        self.read_pos += amt;
    }

    /// Get a reference to a buffer to write to
    fn poll_sink_buf(&mut self, cx: &mut Context<'_>, fd: RawFd) -> Poll<io::Result<&mut buf::SliceMut>> {
        loop {
            match &mut self.write {
                Write::Idle => {
                    // If the buffer is full, we must flush.
                    if self.write_buf.capacity() == 0 {
                        ready!(self.poll_flush(cx, fd))?;
                        assert!(self.write_buf.capacity() > 0);
                    }

                    return Poll::Ready(Ok(&mut self.write_buf));
                }
                Write::Writing(_) => {
                    ready!(self.poll_flush(cx, fd))?;
                }
            }
        }
    }

    fn produce(&mut self, amt: usize) {
        self.write_buf.set_begin(self.write_buf.begin() + amt);
    }

    fn flush_if_full(&mut self, fd: RawFd) -> io::Result<()> {
        if self.write_buf.capacity() == 0 {
            self.write_buf.set_begin(0);
            self.submit_write(fd)?;
        }

        Ok(())
    }

    /// Flush all buffered data to the socket
    fn poll_flush(&mut self, cx: &mut Context<'_>, fd: RawFd) -> Poll<io::Result<()>> {
        loop {
            match &mut self.write {
                Write::Idle => {
                    if self.write_buf.begin() == 0 {
                        self.write_buf.get_inner_mut().clear();
                        return Poll::Ready(Ok(()));
                    }

                    self.write_buf.set_begin(0);
                    self.submit_write(fd)?;
                }
                Write::Writing(op) => {
                    let (res, buf) = ready!(Pin::new(op).poll_write(cx));
                    self.write_buf = buf;
                    let n = res?;

                    self.write_buf.set_begin(self.write_buf.begin() + n);

                    if self.write_buf.is_empty() {
                        self.write_buf.set_begin(0);
                        self.write_buf.get_inner_mut().clear();
                        self.write = Write::Idle;

                        return Poll::Ready(Ok(()));
                    } else {
                        self.submit_write(fd)?;
                    }
                }
            }
        }
    }

    fn submit_write(&mut self, fd: RawFd) -> io::Result<()> {
        let buf = mem::replace(&mut self.write_buf, Default::default());
        let op = Op::write_at(fd, buf, 0)?;
        self.write = Write::Writing(op);
        Ok(())
    }
}