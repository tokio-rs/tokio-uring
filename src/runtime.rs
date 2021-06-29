use crate::driver::Driver;

use std::future::Future;
use std::io;
use tokio::io::unix::AsyncFd;
use tokio::task::LocalSet;

pub struct Runtime {
    /// io-uring driver
    driver: AsyncFd<Driver>,

    /// LocalSet for !Send tasks
    local: LocalSet,

    /// Tokio runtime, always current-thread
    rt: tokio::runtime::Runtime,
}

impl Runtime {
    pub fn new() -> io::Result<Runtime> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        let local = LocalSet::new();

        let driver = {
            let _guard = rt.enter();
            AsyncFd::new(Driver::new()?)?
        };

        Ok(Runtime { driver, local, rt })
    }

    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        self.driver.get_ref().with(|| {
            let drive = async {
                loop {
                    // Wait for read-readiness
                    let mut guard = self.driver.readable().await.unwrap();
                    self.driver.get_ref().tick();
                    guard.clear_ready();
                }
            };

            tokio::pin!(drive);
            tokio::pin!(future);

            self.rt
                .block_on(self.local.run_until(crate::future::poll_fn(|cx| {
                    assert!(drive.as_mut().poll(cx).is_pending());
                    future.as_mut().poll(cx)
                })))
        })
    }
}
