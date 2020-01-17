use std::cell::UnsafeCell;
use std::mem::transmute;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    pub value: UnsafeCell<T>,
    pub flag: AtomicBool,
}

unsafe impl<T> Send for SpinLock<T> {}
unsafe impl<T> Sync for SpinLock<T> {}

pub struct SpinLockGuard<'a, T: 'a>(&'a SpinLock<T>);

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        SpinLock {
            value: UnsafeCell::new(value),
            flag: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<T> {
        while self.flag.compare_and_swap(false, true, Ordering::Acquire) != false {}

        SpinLockGuard(self)
    }

    pub fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        if self.flag.compare_and_swap(false, true, Ordering::Acquire) == false {
            Some(SpinLockGuard(self))
        } else {
            None
        }
    }

    pub unsafe fn unsafe_as_ref(&self) -> &T {
        &*self.value.get()
    }

    pub unsafe fn force_unlock(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

impl<'a, T> SpinLockGuard<'a, T> {
    #[allow(dead_code)]
    pub fn unwrap(self) -> Self {
        self
    }
}

impl<'a, T> SpinLockGuard<'a, *mut T> {
    #[allow(dead_code)]
    pub unsafe fn as_ref(self) -> SpinLockGuard<'a, &'a T> {
        transmute(self)
    }
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.0.flag.store(false, Ordering::Release);
    }
}

impl<'a, T> Deref for SpinLockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { transmute(self.0.value.get()) }
    }
}

impl<'a, T> DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { transmute(self.0.value.get()) }
    }
}
