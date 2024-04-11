
macro_rules! unit_err {
    ($name:ident: $msg:expr) => {
        #[derive(Debug, Clone)]
        pub struct $name;
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                $msg.fmt(f)
            }
        }
        impl std::error::Error for $name { }
    };
}

use std::sync::{Arc, Mutex};

pub(crate) use unit_err;

/*
pub use heavy_rw::*;
mod heavy_rw {
    use std::{panic::Location, sync::{Mutex, RwLock, RwLockReadGuard}};


    pub struct HeavyRwLock<T> {
        lock: RwLock<T>,
        readers: Mutex<Vec<Location<'static>>>,
        writer: Mutex<Option<Location<'static>>>,
    }

    pub struct HeavyRwReadGuard<T> {
        guard: 
    }

    impl<T> HeavyRwLock<T> {
        #[track_caller]
        fn read(&self) -> RwLockReadGuard<T> {
            let mut rl = self.readers.lock().unwrap();
            rl.push(*Location::caller());
            drop(rl);
        }
    }
}
*/

/// TODO: improve this implementation
pub struct AtomicArc<T> {
    inner: Mutex<Option<Arc<T>>>,
}

impl<T> AtomicArc<T> {
    pub const fn new() -> Self {
        AtomicArc { inner: Mutex::new(None) }
    }

    pub fn set(&self, item: Arc<T>) {
        *self.inner.lock().expect("implementation of Arc::clone failed") = Some(item)
    }

    pub fn get(&self) -> Option<Arc<T>> {
        self.inner.lock().expect("implementation of Arc::clone failed").clone()
    }
}
