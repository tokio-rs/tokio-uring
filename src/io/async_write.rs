use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pub trait AsyncWriteOwned {
    fn write_owned(&mut self, buf: Vec<u8>) -> OwnedWrite;
}

pub struct OwnedWrite {
    data: *const (),
    vtable: OwnedWriteVTable,
}

struct OwnedWriteVTable {
    poll: fn(*const (), cx: &mut Context<'_>) -> Poll<(Vec<u8>, io::Result<usize>)>,
    drop: fn(*const ()),
}

impl OwnedWrite {
    pub unsafe fn new(
        data: *const (),
        poll: fn(*const (), cx: &mut Context<'_>) -> Poll<(Vec<u8>, io::Result<usize>)>,
        drop: fn(*const ()),
    ) -> Self {
        let vtable = OwnedWriteVTable { poll, drop };

        Self { data, vtable }
    }
}

impl Drop for OwnedWrite {
    fn drop(&mut self) {
        let custom_drop = self.vtable.drop;
        custom_drop(self.data)
    }
}

impl Future for OwnedWrite {
    type Output = (Vec<u8>, io::Result<usize>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let poll = self.vtable.poll;
        poll(self.data, cx)
    }
}
