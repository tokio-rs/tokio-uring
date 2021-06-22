use std::ffi::CString;
use std::io;
use std::path::Path;

/*
/// Prepare a SQE
pub(super) fn push_sqe(uring: &mut IoUring, &squeue::Entry) -> io::Result<()> {
    if uring.sq_space_left() == 0 {
        uring.submit_sqes()?;
    }

    Ok(uring.prepare_sqe().expect("unimplemented: handle busy SQ"))
}
*/

pub(super) fn cstr(p: &Path) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    Ok(CString::new(p.as_os_str().as_bytes())?)
}

// use std::task::{Context, Waker};

// #[derive(Default)]
// pub(crate) struct MaybeWaker(Option<Waker>);

// impl MaybeWaker {
//     pub(crate) fn new(waker: Waker) -> MaybeWaker {
//         MaybeWaker(Some(waker))
//     }

//     pub(crate) fn set(&mut self, cx: &mut Context<'_>) {
//         match &mut self.0 {
//             // Waker is already stored
//             Some(w) if w.will_wake(cx.waker()) => {}
//             // Waker needs to be stored
//             waker => *waker = Some(cx.waker().clone()),
//         }
//     }

//     pub(crate) fn wake(&mut self) {
//         if let Some(waker) = self.0.take() {
//             waker.wake();
//         }
//     }
// }
