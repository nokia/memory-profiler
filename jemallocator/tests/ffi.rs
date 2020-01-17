extern crate jemalloc_sys as ffi;
extern crate jemallocator;
extern crate libc;

use std::mem;
use std::ptr;

use jemallocator::Jemalloc;
use libc::{c_char, c_void};

#[global_allocator]
static A: Jemalloc = Jemalloc;

#[test]
fn test_basic_alloc() {
    unsafe {
        let exp_size = ffi::nallocx(100, 0);
        assert!(exp_size >= 100);

        let mut ptr = ffi::mallocx(100, 0);
        assert!(!ptr.is_null());
        assert_eq!(exp_size, ffi::malloc_usable_size(ptr));
        ptr = ffi::rallocx(ptr, 50, 0);
        let size = ffi::xallocx(ptr, 30, 20, 0);
        assert!(size >= 50);
        ffi::sdallocx(ptr, 50, 0);
    }
}

#[test]
fn test_mallctl() {
    let ptr = unsafe { ffi::mallocx(100, 0) };
    let mut allocated: usize = 0;
    let mut val_len = mem::size_of_val(&allocated);
    let field = "stats.allocated\0";
    let mut code;
    code = unsafe {
        ffi::mallctl(
            field.as_ptr() as *const _,
            &mut allocated as *mut _ as *mut c_void,
            &mut val_len,
            ptr::null_mut(),
            0,
        )
    };
    assert_eq!(code, 0);
    assert!(allocated > 0);

    let mut mib = [0, 0];
    let mut mib_len = 2;
    code = unsafe {
        ffi::mallctlnametomib(field.as_ptr() as *const _, mib.as_mut_ptr(), &mut mib_len)
    };
    assert_eq!(code, 0);
    let mut allocated_by_mib = 0;
    let code = unsafe {
        ffi::mallctlbymib(
            mib.as_ptr(),
            mib_len,
            &mut allocated_by_mib as *mut _ as *mut c_void,
            &mut val_len,
            ptr::null_mut(),
            0,
        )
    };
    assert_eq!(code, 0);
    assert_eq!(allocated_by_mib, allocated);

    unsafe { ffi::sdallocx(ptr, 100, 0) };
}

#[test]
fn test_stats() {
    struct PrintCtx {
        called_times: usize,
    }

    extern "C" fn write_cb(ctx: *mut c_void, _: *const c_char) {
        let print_ctx = unsafe { &mut *(ctx as *mut PrintCtx) };
        print_ctx.called_times += 1;
    }

    let mut ctx = PrintCtx { called_times: 0 };
    unsafe {
        ffi::malloc_stats_print(write_cb, &mut ctx as *mut _ as *mut c_void, ptr::null());
    }
    assert_ne!(
        ctx.called_times, 0,
        "print should be triggered at lease once."
    );
}
