use crate::runtime::driver;
use crate::runtime::driver::{Handle, WeakHandle};
use std::cell::RefCell;

/// Owns the driver and resides in thread-local storage.
pub(crate) struct RuntimeContext {
    driver: RefCell<Option<driver::Handle>>,
}

impl RuntimeContext {
    /// Construct the context with an uninitialized driver.
    pub(crate) const fn new() -> Self {
        Self {
            driver: RefCell::new(None),
        }
    }

    /// Initialize the driver.
    pub(crate) fn set_handle(&self, handle: Handle) {
        let mut guard = self.driver.borrow_mut();

        assert!(guard.is_none(), "Attempted to initialize the driver twice");

        *guard = Some(handle);
    }

    pub(crate) fn unset_driver(&self) {
        let mut guard = self.driver.borrow_mut();

        assert!(guard.is_some(), "Attempted to clear nonexistent driver");

        *guard = None;
    }

    /// Check if driver is initialized
    #[allow(dead_code)]
    pub(crate) fn is_set(&self) -> bool {
        self.driver
            .try_borrow()
            .map(|b| b.is_some())
            .unwrap_or(false)
    }

    pub(crate) fn handle(&self) -> Option<Handle> {
        self.driver.borrow().clone()
    }

    #[allow(dead_code)]
    pub(crate) fn weak(&self) -> Option<WeakHandle> {
        self.driver.borrow().as_ref().map(Into::into)
    }
}
