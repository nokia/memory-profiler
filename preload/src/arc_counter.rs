use std::mem;
use std::ptr::NonNull;
use std::sync::atomic::{self, AtomicUsize, Ordering};

pub struct ArcCounter(NonNull<AtomicUsize>);
unsafe impl Send for ArcCounter {}

impl ArcCounter {
    pub fn new() -> Self {
        let ptr = Box::into_raw(Box::new(AtomicUsize::new(1)));
        unsafe { ArcCounter(NonNull::new_unchecked(ptr)) }
    }

    #[inline]
    fn inner(&self) -> &AtomicUsize {
        unsafe { self.0.as_ref() }
    }

    #[inline]
    pub fn get(&self) -> usize {
        self.inner().load(Ordering::Relaxed)
    }

    #[inline(never)]
    unsafe fn drop_slow(&mut self) {
        mem::drop(Box::from_raw(self.0.as_ptr()));
    }

    pub unsafe fn add(&self, value: usize) {
        self.inner().fetch_add(value, Ordering::SeqCst);
    }

    pub unsafe fn sub(&self, value: usize) {
        self.inner().fetch_sub(value, Ordering::SeqCst);
    }
}

impl Clone for ArcCounter {
    #[inline]
    fn clone(&self) -> Self {
        self.inner().fetch_add(1, Ordering::Relaxed);
        ArcCounter(self.0)
    }
}

impl Drop for ArcCounter {
    #[inline]
    fn drop(&mut self) {
        if self.inner().fetch_sub(1, Ordering::Release) != 1 {
            return;
        }

        atomic::fence(Ordering::Acquire);
        unsafe {
            self.drop_slow();
        }
    }
}
