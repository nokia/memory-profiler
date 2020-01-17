// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Bindings for jemalloc as an allocator
//!
//! This crate provides bindings to jemalloc as a memory allocator for Rust.
//! This crate mainly exports, one type, `Jemalloc`, which implements the
//! `GlobalAlloc` trait and optionally the `Alloc` trait,
//! and is suitable both as a memory allocator and as a global allocator.

#![cfg_attr(feature = "alloc_trait", feature(allocator_api))]
#![deny(missing_docs)]
#![no_std]

extern crate jemalloc_sys;
extern crate libc;

#[cfg(feature = "alloc_trait")]
use core::alloc::{Alloc, AllocErr, CannotReallocInPlace, Excess};
use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use core::ptr;
#[cfg(feature = "alloc_trait")]
use core::ptr::NonNull;

use libc::{c_int, c_void};

// The minimum alignment guaranteed by the architecture. This value is used to
// add fast paths for low alignment values. In practice, the alignment is a
// constant at the call site and the branch will be optimized out.
#[cfg(all(any(
    target_arch = "arm",
    target_arch = "mips",
    target_arch = "mipsel",
    target_arch = "powerpc"
)))]
const MIN_ALIGN: usize = 8;
#[cfg(all(any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "powerpc64",
    target_arch = "powerpc64le",
    target_arch = "mips64",
    target_arch = "s390x",
    target_arch = "sparc64"
)))]
const MIN_ALIGN: usize = 16;

fn layout_to_flags(align: usize, size: usize) -> c_int {
    // If our alignment is less than the minimum alignment, then we may not
    // have to pass special flags asking for a higher alignment. If the
    // alignment is greater than the size, however, then this hits a sort of odd
    // case where we still need to ask for a custom alignment. See #25 for more
    // info.
    if align <= MIN_ALIGN && align <= size {
        0
    } else {
        ffi::MALLOCX_ALIGN(align)
    }
}

/// Handle to the jemalloc allocator
///
/// This type implements the `GlobalAllocAlloc` trait, allowing usage a global allocator.
///
/// When the `alloc_trait` feature of this crate is enabled, it also implements the `Alloc` trait,
/// allowing usage in collections.
pub struct Jemalloc;

unsafe impl GlobalAlloc for Jemalloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let flags = layout_to_flags(layout.align(), layout.size());
        let ptr = ffi::mallocx(layout.size(), flags);
        ptr as *mut u8
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = if layout.align() <= MIN_ALIGN && layout.align() <= layout.size() {
            ffi::calloc(1, layout.size())
        } else {
            let flags = layout_to_flags(layout.align(), layout.size()) | ffi::MALLOCX_ZERO;
            ffi::mallocx(layout.size(), flags)
        };
        ptr as *mut u8
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let flags = layout_to_flags(layout.align(), layout.size());
        ffi::sdallocx(ptr as *mut c_void, layout.size(), flags)
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let flags = layout_to_flags(layout.align(), new_size);
        let ptr = ffi::rallocx(ptr as *mut c_void, new_size, flags);
        ptr as *mut u8
    }
}

#[cfg(feature = "alloc_trait")]
unsafe impl Alloc for Jemalloc {
    #[inline]
    unsafe fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, AllocErr> {
        NonNull::new(GlobalAlloc::alloc(self, layout)).ok_or(AllocErr)
    }

    #[inline]
    unsafe fn alloc_zeroed(&mut self, layout: Layout) -> Result<NonNull<u8>, AllocErr> {
        NonNull::new(GlobalAlloc::alloc_zeroed(self, layout)).ok_or(AllocErr)
    }

    #[inline]
    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        GlobalAlloc::dealloc(self, ptr.as_ptr(), layout)
    }

    #[inline]
    unsafe fn realloc(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, AllocErr> {
        NonNull::new(GlobalAlloc::realloc(self, ptr.as_ptr(), layout, new_size)).ok_or(AllocErr)
    }

    #[inline]
    unsafe fn alloc_excess(&mut self, layout: Layout) -> Result<Excess, AllocErr> {
        let flags = layout_to_flags(layout.align(), layout.size());
        let ptr = ffi::mallocx(layout.size(), flags);
        if let Some(nonnull) = NonNull::new(ptr as *mut u8) {
            let excess = ffi::nallocx(layout.size(), flags);
            Ok(Excess(nonnull, excess))
        } else {
            Err(AllocErr)
        }
    }

    #[inline]
    unsafe fn realloc_excess(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<Excess, AllocErr> {
        let flags = layout_to_flags(layout.align(), new_size);
        let ptr = ffi::rallocx(ptr.cast().as_ptr(), new_size, flags);
        if let Some(nonnull) = NonNull::new(ptr as *mut u8) {
            let excess = ffi::nallocx(new_size, flags);
            Ok(Excess(nonnull, excess))
        } else {
            Err(AllocErr)
        }
    }

    #[inline]
    fn usable_size(&self, layout: &Layout) -> (usize, usize) {
        let flags = layout_to_flags(layout.align(), layout.size());
        unsafe {
            let max = ffi::nallocx(layout.size(), flags);
            (layout.size(), max)
        }
    }

    #[inline]
    unsafe fn grow_in_place(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<(), CannotReallocInPlace> {
        self.shrink_in_place(ptr, layout, new_size)
    }

    #[inline]
    unsafe fn shrink_in_place(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<(), CannotReallocInPlace> {
        let flags = layout_to_flags(layout.align(), new_size);
        let shrunk_size = ffi::xallocx(ptr.cast().as_ptr(), new_size, 0, flags);
        debug_assert!(shrunk_size >= new_size);
        Ok(())
    }
}

/// Return the usable size of the allocation pointed to by ptr.
///
/// The return value may be larger than the size that was requested during allocation.
/// This function is not a mechanism for in-place `realloc()`;
/// rather it is provided solely as a tool for introspection purposes.
/// Any discrepancy between the requested allocation size
/// and the size reported by this function should not be depended on,
/// since such behavior is entirely implementation-dependent.
///
/// # Unsafety
///
/// `ptr` must have been allocated by `Jemalloc` and must not have been freed yet.
pub unsafe fn usable_size<T>(ptr: *const T) -> usize {
    ffi::malloc_usable_size(ptr as *const c_void)
}

/// Fetch the value of options `name`.
///
/// Please note that if you want to fetch a string, use char* instead of &str or
/// cstring.
pub unsafe fn mallctl_fetch<T>(name: &[u8], t: &mut T) -> Result<(), i32> {
    // make sure name is a valid c string.
    if name.is_empty() || *name.last().unwrap() != 0 {
        return Err(libc::EINVAL);
    }
    let mut t_size = mem::size_of::<T>();
    let t_ptr = t as *mut T as *mut _;
    let code = ffi::mallctl(
        name.as_ptr() as *const _,
        t_ptr,
        &mut t_size,
        ptr::null_mut(),
        0,
    );
    if code != 0 {
        return Err(code);
    }
    Ok(())
}

/// Set a value to option `name`.
///
/// Please note that if you want to set a string, use char* instead of &str or
/// cstring.
pub unsafe fn mallctl_set<T>(name: &[u8], mut t: T) -> Result<(), i32> {
    // make sure name is a valid c string.
    if name.is_empty() || *name.last().unwrap() != 0 {
        return Err(libc::EINVAL);
    }
    let size = mem::size_of::<T>();
    let code = ffi::mallctl(
        name.as_ptr() as *const _,
        ptr::null_mut(),
        ptr::null_mut(),
        &mut t as *mut T as *mut _,
        size,
    );
    if code != 0 {
        return Err(code);
    }
    Ok(())
}

/// Raw bindings to jemalloc
pub mod ffi {
    pub use jemalloc_sys::*;
}
