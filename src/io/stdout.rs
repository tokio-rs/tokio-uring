/*
use crate::buf;
use crate::driver::Op;

use std::io;
*/
// use std::os::unix::io::RawFd;

pub struct Stdout(());

pub fn stdout() -> Stdout {
    Stdout(())
}

// const STDOUT: RawFd = 1;

impl Stdout {
    /*
    pub async fn write(&self, buf: buf::Slice) -> io::Result<usize> {
        let op = Op::write_at(STDOUT, &buf, 0)?;

        // Await the completion of the event
        let completion = op.await;

        Ok(completion.result? as _)
    }
    */
}