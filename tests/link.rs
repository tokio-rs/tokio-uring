use std::task::Context;

use futures_util::{future::{Either, BoxFuture, join_all}, Future, FutureExt};
use io_uring::squeue::Flags;
use tempfile::NamedTempFile;
use tokio_uring::{fs::File, buf::IoBufMut, BufResult};
use std::task::Poll;

struct UnsafeBuffer {
	addr : *mut u8
}

unsafe impl tokio_uring::buf::IoBuf for UnsafeBuffer {
    fn stable_ptr(&self) -> *const u8 {
        self.addr
    }

    fn bytes_init(&self) -> usize {
        std::mem::size_of::<u8>()
    }

    fn bytes_total(&self) -> usize {
        std::mem::size_of::<u8>()
    }
}

unsafe impl IoBufMut for UnsafeBuffer {
    fn stable_mut_ptr(&mut self) -> *mut u8 {
		self.addr
    }
    unsafe fn set_init(&mut self, _pos: usize) {}
}

fn tempfile() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

// struct MyFuture<'a> {
// 	is_read: bool,
// 	file: &'a File,
// 	offset: usize,
// 	addr : *mut u8,

// 	future: Option<BoxFuture<'static, BufResult<usize, UnsafeBuffer>>>
// }

// impl Future for MyFuture<'_> {
// 	type Output = ();
// 	fn poll(self: std::pin::Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<()> {
// 		let buf = UnsafeBuffer { addr: self.addr, };		
// 		if self.is_read {
// 			if self.future.is_none() {
// 				self.future = Some(Box::pin(self.file.read_at_with_flags(buf, 0, Flags::IO_LINK)));
// 			}
// 			// let result = self.file.read_at_with_flags(buf, 0, Flags::IO_LINK);
// 			Poll::Ready(())
// 		} else {
// 			let result = self.file.write_at_with_flags(buf, 0, Flags::IO_LINK).submit();
// 			Poll::Ready(())
// 		}
// 	}
// }


// fn read(file: &File, addr: *mut u8) -> MyFuture {
// 	MyFuture { is_read: true, file, offset: 0, addr, future: None,}

// 	// file.read_at_with_flags(buf, 0, Flags::IO_LINK).await;
// }

// fn write(file: &File, addr: *mut u8) -> MyFuture {
// 	MyFuture { is_read: false, file, offset: 0, addr, future: None,}
// 	// let buf = UnsafeBuffer { address: buffer, };
// 	// file.write_at_with_flags(buf, 0, Flags::IO_LINK).submit().await;
// }

#[test]
fn multiple_write() {
    tokio_uring::start(async {
        let tempfile = tempfile();
        let file = File::open(tempfile.path()).await.unwrap();
		let mut tasks = Vec::new();
		for i in 0..100 {
			let buf = i.to_string();
			let write_task = file.write_at_with_flags(buf.into_bytes(), 0, Flags::IO_LINK).submit();

			tasks.push(write_task);
		}
		let _ = futures_util::future::join_all(tasks).await;

		let buf = vec![0u8, 10];
		let (_, result) = file.read_at(buf, 0).await;
		let str = String::from_utf8(result).unwrap();
		assert_eq!(&str, "100");
		
    });
}


#[test]
fn mix_read_write() {
    tokio_uring::start(async {
        let mut tempfile = tempfile();
        let file = File::open(tempfile.path()).await.unwrap();
		let mut buffer = 0u8;

		let mut tasks = Vec::new();
		for i in 0..100 {

			let buf = UnsafeBuffer { addr: std::ptr::addr_of_mut!(buffer), };
			let write_task = file.write_at_with_flags(buf, 0, Flags::IO_LINK).submit().await;

			tasks.push(write_task);
			
			// let read_task = read(&file, std::ptr::addr_of_mut!(buffer));
			// tasks.push(read_task);
			// let write_task = write(&file, std::ptr::addr_of_mut!(buffer));
			// tasks.push(write_task);
			
		}

		
    });
}
