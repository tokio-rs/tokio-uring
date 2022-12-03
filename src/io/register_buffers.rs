use crate::buf::fixed::FixedBuffers;
use crate::runtime::CONTEXT;

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

pub(crate) fn register_buffers(buffers: Rc<RefCell<dyn FixedBuffers>>) -> io::Result<()> {
    CONTEXT.with(|cx| {
        cx.with_driver_mut(|driver| {
            driver
                .uring
                .submitter()
                .register_buffers(buffers.borrow().iovecs())?;
            driver.fixed_buffers = Some(buffers);
            Ok(())
        })
    })
}

pub(crate) fn unregister_buffers(buffers: Rc<RefCell<dyn FixedBuffers>>) -> io::Result<()> {
    CONTEXT.with(|cx| {
        cx.with_driver_mut(|driver| {
            if let Some(currently_registered) = &driver.fixed_buffers {
                if Rc::ptr_eq(&buffers, currently_registered) {
                    driver.uring.submitter().unregister_buffers()?;
                    driver.fixed_buffers = None;
                    return Ok(());
                }
            }
            Err(io::Error::new(
                io::ErrorKind::Other,
                "fixed buffers are not currently registered",
            ))
        })
    })
}
