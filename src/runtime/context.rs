use crate::driver::Driver;
use crate::util::PhantomUnsendUnsync;
use std::cell::RefCell;
use std::marker::PhantomData;

/// Owns the driver and resides in thread-local storage.
pub(crate) struct RuntimeContext {
    driver: RefCell<Option<Driver>>,
    _phantom: PhantomUnsendUnsync,
}

impl RuntimeContext {
    /// Construct the context with an uninitialized driver.
    pub(crate) const fn new() -> Self {
        Self {
            driver: RefCell::new(None),
            _phantom: PhantomData,
        }
    }

    /// Initialize the driver.
    pub(crate) fn set_driver(&self, driver: Driver) {
        let mut guard = self.driver.borrow_mut();

        assert!(guard.is_none(), "Attempted to initialize the driver twice");

        *guard = Some(driver);
    }

    pub(crate) fn unset_driver(&self) {
        let mut guard = self.driver.borrow_mut();

        assert!(guard.is_some(), "Attempted to clear nonexistent driver");

        *guard = None;
    }

    /// Check if driver is initialized
    pub(crate) fn is_set(&self) -> bool {
        self.driver
            .try_borrow()
            .map(|b| b.is_some())
            .unwrap_or(false)
    }

    /// Execute a function which requires mutable access to the driver.
    pub(crate) fn with_driver_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Driver) -> R,
    {
        let mut guard = self.driver.borrow_mut();

        let driver = guard
            .as_mut()
            .expect("Attempted to access driver in invalid context");

        f(driver)
    }
}
