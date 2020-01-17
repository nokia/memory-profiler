//! Benchmarks the cost of the different allocation functions by doing a
//! roundtrip (allocate, deallocate).
#![feature(test, global_allocator, allocator_api)]

extern crate jemallocator;
extern crate libc;
extern crate test;

use jemallocator::Jemalloc;
use libc::c_int;
use std::heap::{Alloc, Excess, Layout};
use std::ptr;
use test::Bencher;

#[global_allocator]
static A: Jemalloc = Jemalloc;

// FIXME: replace with utils::mallocx_align
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
    target_arch = "powerpc64le"
)))]
const MIN_ALIGN: usize = 16;

// FIXME: replace with utils::mallocx_align
fn mallocx_align(a: usize) -> c_int {
    a.trailing_zeros() as c_int
}

fn layout_to_flags(layout: &Layout) -> c_int {
    if layout.align() <= MIN_ALIGN && layout.align() <= layout.size() {
        0
    } else {
        mallocx_align(layout.align())
    }
}

macro_rules! rt_mallocx {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                use jemallocator::ffi as jemalloc;
                let flags = layout_to_flags(&Layout::from_size_align($size, $align).unwrap());
                let ptr = jemalloc::mallocx($size, flags);
                test::black_box(ptr);
                jemalloc::sdallocx(ptr, $size, flags);
            });
        }
    };
}

macro_rules! rt_mallocx_nallocx {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                use jemallocator::ffi as jemalloc;
                let flags = layout_to_flags(&Layout::from_size_align($size, $align).unwrap());
                let ptr = jemalloc::mallocx($size, flags);
                test::black_box(ptr);
                let rsz = jemalloc::nallocx($size, flags);
                test::black_box(rsz);
                jemalloc::sdallocx(ptr, rsz, flags);
            });
        }
    };
}

macro_rules! rt_alloc_layout_checked {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                let layout = Layout::from_size_align($size, $align).unwrap();
                let ptr = Jemalloc.alloc(layout.clone()).unwrap();
                test::black_box(ptr);
                Jemalloc.dealloc(ptr, layout);
            });
        }
    };
}

macro_rules! rt_alloc_layout_unchecked {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                let layout = Layout::from_size_align_unchecked($size, $align);
                let ptr = Jemalloc.alloc(layout.clone()).unwrap();
                test::black_box(ptr);
                Jemalloc.dealloc(ptr, layout);
            });
        }
    };
}

macro_rules! rt_alloc_excess_unused {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                let layout = Layout::from_size_align($size, $align).unwrap();
                let Excess(ptr, _) = Jemalloc.alloc_excess(layout.clone()).unwrap();
                test::black_box(ptr);
                Jemalloc.dealloc(ptr, layout);
            });
        }
    };
}

macro_rules! rt_alloc_excess_used {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                let layout = Layout::from_size_align($size, $align).unwrap();
                let Excess(ptr, excess) = Jemalloc.alloc_excess(layout.clone()).unwrap();
                test::black_box(ptr);
                test::black_box(excess);
                Jemalloc.dealloc(ptr, layout);
            });
        }
    };
}

macro_rules! rt_mallocx_zeroed {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                use jemallocator::ffi as jemalloc;
                let flags = layout_to_flags(&Layout::from_size_align($size, $align).unwrap());
                let ptr = jemalloc::mallocx($size, flags | jemalloc::MALLOCX_ZERO);
                test::black_box(ptr);
                jemalloc::sdallocx(ptr, $size, flags);
            });
        }
    };
}

macro_rules! rt_calloc {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                use jemallocator::ffi as jemalloc;
                let flags = layout_to_flags(&Layout::from_size_align($size, $align).unwrap());
                test::black_box(flags);
                let ptr = jemalloc::calloc(1, $size);
                test::black_box(ptr);
                jemalloc::sdallocx(ptr, $size, 0);
            });
        }
    };
}

macro_rules! rt_realloc_naive {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                let layout = Layout::from_size_align($size, $align).unwrap();
                let ptr = Jemalloc.alloc(layout.clone()).unwrap();
                test::black_box(ptr);

                // navie realloc:
                let new_layout = Layout::from_size_align(2 * $size, $align).unwrap();
                let ptr = {
                    let new_ptr = Jemalloc.alloc(new_layout.clone()).unwrap();
                    ptr::copy_nonoverlapping(ptr as *const u8, new_ptr, layout.size());
                    Jemalloc.dealloc(ptr, layout);
                    new_ptr
                };
                test::black_box(ptr);

                Jemalloc.dealloc(ptr, new_layout);
            });
        }
    };
}

macro_rules! rt_realloc {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                let layout = Layout::from_size_align($size, $align).unwrap();
                let ptr = Jemalloc.alloc(layout.clone()).unwrap();
                test::black_box(ptr);

                let new_layout = Layout::from_size_align(2 * $size, $align).unwrap();
                let ptr = Jemalloc.realloc(ptr, layout, new_layout.clone()).unwrap();
                test::black_box(ptr);

                Jemalloc.dealloc(ptr, new_layout);
            });
        }
    };
}

macro_rules! rt_realloc_excess_unused {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                let layout = Layout::from_size_align($size, $align).unwrap();
                let ptr = Jemalloc.alloc(layout.clone()).unwrap();
                test::black_box(ptr);

                let new_layout = Layout::from_size_align(2 * $size, $align).unwrap();
                let Excess(ptr, _) = Jemalloc
                    .realloc_excess(ptr, layout, new_layout.clone())
                    .unwrap();
                test::black_box(ptr);

                Jemalloc.dealloc(ptr, new_layout);
            });
        }
    };
}

macro_rules! rt_realloc_excess_used {
    ($name:ident, $size:expr, $align:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| unsafe {
                let layout = Layout::from_size_align($size, $align).unwrap();
                let ptr = Jemalloc.alloc(layout.clone()).unwrap();
                test::black_box(ptr);

                let new_layout = Layout::from_size_align(2 * $size, $align).unwrap();
                let Excess(ptr, excess) = Jemalloc
                    .realloc_excess(ptr, layout, new_layout.clone())
                    .unwrap();
                test::black_box(ptr);
                test::black_box(excess);

                Jemalloc.dealloc(ptr, new_layout);
            });
        }
    };
}

// 1 byte alignment

// Powers of two
rt_calloc!(rt_pow2_1bytes_1align_calloc, 1, 1);
rt_mallocx!(rt_pow2_1bytes_1align_mallocx, 1, 1);
rt_mallocx_zeroed!(rt_pow2_1bytes_1align_mallocx_zeroed, 1, 1);
rt_mallocx_nallocx!(rt_pow2_1bytes_1align_mallocx_nallocx, 1, 1);
rt_alloc_layout_checked!(rt_pow2_1bytes_1align_alloc_layout_checked, 1, 1);
rt_alloc_layout_unchecked!(rt_pow2_1bytes_1align_alloc_layout_unchecked, 1, 1);
rt_alloc_excess_unused!(rt_pow2_1bytes_1align_alloc_excess_unused, 1, 1);
rt_alloc_excess_used!(rt_pow2_1bytes_1align_alloc_excess_used, 1, 1);
rt_realloc_naive!(rt_pow2_1bytes_1align_realloc_naive, 1, 1);
rt_realloc!(rt_pow2_1bytes_1align_realloc, 1, 1);
rt_realloc_excess_unused!(rt_pow2_1bytes_1align_realloc_excess_unused, 1, 1);
rt_realloc_excess_used!(rt_pow2_1bytes_1align_realloc_excess_used, 1, 1);

rt_calloc!(rt_pow2_2bytes_1align_calloc, 2, 1);
rt_mallocx!(rt_pow2_2bytes_1align_mallocx, 2, 1);
rt_mallocx_zeroed!(rt_pow2_2bytes_1align_mallocx_zeroed, 2, 1);
rt_mallocx_nallocx!(rt_pow2_2bytes_1align_mallocx_nallocx, 2, 1);
rt_alloc_layout_checked!(rt_pow2_2bytes_1align_alloc_layout_checked, 2, 1);
rt_alloc_layout_unchecked!(rt_pow2_2bytes_1align_alloc_layout_unchecked, 2, 1);
rt_alloc_excess_unused!(rt_pow2_2bytes_1align_alloc_excess_unused, 2, 1);
rt_alloc_excess_used!(rt_pow2_2bytes_1align_alloc_excess_used, 2, 1);
rt_realloc_naive!(rt_pow2_2bytes_1align_realloc_naive, 2, 1);
rt_realloc!(rt_pow2_2bytes_1align_realloc, 2, 1);
rt_realloc_excess_unused!(rt_pow2_2bytes_1align_realloc_excess_unused, 2, 1);
rt_realloc_excess_used!(rt_pow2_2bytes_1align_realloc_excess_used, 2, 1);

rt_calloc!(rt_pow2_4bytes_1align_calloc, 4, 1);
rt_mallocx!(rt_pow2_4bytes_1align_mallocx, 4, 1);
rt_mallocx_zeroed!(rt_pow2_4bytes_1align_mallocx_zeroed, 4, 1);
rt_mallocx_nallocx!(rt_pow2_4bytes_1align_mallocx_nallocx, 4, 1);
rt_alloc_layout_checked!(rt_pow2_4bytes_1align_alloc_layout_checked, 4, 1);
rt_alloc_layout_unchecked!(rt_pow2_4bytes_1align_alloc_layout_unchecked, 4, 1);
rt_alloc_excess_unused!(rt_pow2_4bytes_1align_alloc_excess_unused, 4, 1);
rt_alloc_excess_used!(rt_pow2_4bytes_1align_alloc_excess_used, 4, 1);
rt_realloc_naive!(rt_pow2_4bytes_1align_realloc_naive, 4, 1);
rt_realloc!(rt_pow2_4bytes_1align_realloc, 4, 1);
rt_realloc_excess_unused!(rt_pow2_4bytes_1align_realloc_excess_unused, 4, 1);
rt_realloc_excess_used!(rt_pow2_4bytes_1align_realloc_excess_used, 4, 1);

rt_calloc!(rt_pow2_8bytes_1align_calloc, 8, 1);
rt_mallocx!(rt_pow2_8bytes_1align_mallocx, 8, 1);
rt_mallocx_zeroed!(rt_pow2_8bytes_1align_mallocx_zeroed, 8, 1);
rt_mallocx_nallocx!(rt_pow2_8bytes_1align_mallocx_nallocx, 8, 1);
rt_alloc_layout_checked!(rt_pow2_8bytes_1align_alloc_layout_checked, 8, 1);
rt_alloc_layout_unchecked!(rt_pow2_8bytes_1align_alloc_layout_unchecked, 8, 1);
rt_alloc_excess_unused!(rt_pow2_8bytes_1align_alloc_excess_unused, 8, 1);
rt_alloc_excess_used!(rt_pow2_8bytes_1align_alloc_excess_used, 8, 1);
rt_realloc_naive!(rt_pow2_8bytes_1align_realloc_naive, 8, 1);
rt_realloc!(rt_pow2_8bytes_1align_realloc, 8, 1);
rt_realloc_excess_unused!(rt_pow2_8bytes_1align_realloc_excess_unused, 8, 1);
rt_realloc_excess_used!(rt_pow2_8bytes_1align_realloc_excess_used, 8, 1);

rt_calloc!(rt_pow2_16bytes_1align_calloc, 16, 1);
rt_mallocx!(rt_pow2_16bytes_1align_mallocx, 16, 1);
rt_mallocx_zeroed!(rt_pow2_16bytes_1align_mallocx_zeroed, 16, 1);
rt_mallocx_nallocx!(rt_pow2_16bytes_1align_mallocx_nallocx, 16, 1);
rt_alloc_layout_checked!(rt_pow2_16bytes_1align_alloc_layout_checked, 16, 1);
rt_alloc_layout_unchecked!(rt_pow2_16bytes_1align_alloc_layout_unchecked, 16, 1);
rt_alloc_excess_unused!(rt_pow2_16bytes_1align_alloc_excess_unused, 16, 1);
rt_alloc_excess_used!(rt_pow2_16bytes_1align_alloc_excess_used, 16, 1);
rt_realloc_naive!(rt_pow2_16bytes_1align_realloc_naive, 16, 1);
rt_realloc!(rt_pow2_16bytes_1align_realloc, 16, 1);
rt_realloc_excess_unused!(rt_pow2_16bytes_1align_realloc_excess_unused, 16, 1);
rt_realloc_excess_used!(rt_pow2_16bytes_1align_realloc_excess_used, 16, 1);

rt_calloc!(rt_pow2_32bytes_1align_calloc, 32, 1);
rt_mallocx!(rt_pow2_32bytes_1align_mallocx, 32, 1);
rt_mallocx_zeroed!(rt_pow2_32bytes_1align_mallocx_zeroed, 32, 1);
rt_mallocx_nallocx!(rt_pow2_32bytes_1align_mallocx_nallocx, 32, 1);
rt_alloc_layout_checked!(rt_pow2_32bytes_1align_alloc_layout_checked, 32, 1);
rt_alloc_layout_unchecked!(rt_pow2_32bytes_1align_alloc_layout_unchecked, 32, 1);
rt_alloc_excess_unused!(rt_pow2_32bytes_1align_alloc_excess_unused, 32, 1);
rt_alloc_excess_used!(rt_pow2_32bytes_1align_alloc_excess_used, 32, 1);
rt_realloc_naive!(rt_pow2_32bytes_1align_realloc_naive, 32, 1);
rt_realloc!(rt_pow2_32bytes_1align_realloc, 32, 1);
rt_realloc_excess_unused!(rt_pow2_32bytes_1align_realloc_excess_unused, 32, 1);
rt_realloc_excess_used!(rt_pow2_32bytes_1align_realloc_excess_used, 32, 1);

rt_calloc!(rt_pow2_64bytes_1align_calloc, 64, 1);
rt_mallocx!(rt_pow2_64bytes_1align_mallocx, 64, 1);
rt_mallocx_zeroed!(rt_pow2_64bytes_1align_mallocx_zeroed, 64, 1);
rt_mallocx_nallocx!(rt_pow2_64bytes_1align_mallocx_nallocx, 64, 1);
rt_alloc_layout_checked!(rt_pow2_64bytes_1align_alloc_layout_checked, 64, 1);
rt_alloc_layout_unchecked!(rt_pow2_64bytes_1align_alloc_layout_unchecked, 64, 1);
rt_alloc_excess_unused!(rt_pow2_64bytes_1align_alloc_excess_unused, 64, 1);
rt_alloc_excess_used!(rt_pow2_64bytes_1align_alloc_excess_used, 64, 1);
rt_realloc_naive!(rt_pow2_64bytes_1align_realloc_naive, 64, 1);
rt_realloc!(rt_pow2_64bytes_1align_realloc, 64, 1);
rt_realloc_excess_unused!(rt_pow2_64bytes_1align_realloc_excess_unused, 64, 1);
rt_realloc_excess_used!(rt_pow2_64bytes_1align_realloc_excess_used, 64, 1);

rt_calloc!(rt_pow2_128bytes_1align_calloc, 128, 1);
rt_mallocx!(rt_pow2_128bytes_1align_mallocx, 128, 1);
rt_mallocx_zeroed!(rt_pow2_128bytes_1align_mallocx_zeroed, 128, 1);
rt_mallocx_nallocx!(rt_pow2_128bytes_1align_mallocx_nallocx, 128, 1);
rt_alloc_layout_checked!(rt_pow2_128bytes_1align_alloc_layout_checked, 128, 1);
rt_alloc_layout_unchecked!(rt_pow2_128bytes_1align_alloc_layout_unchecked, 128, 1);
rt_alloc_excess_unused!(rt_pow2_128bytes_1align_alloc_excess_unused, 128, 1);
rt_alloc_excess_used!(rt_pow2_128bytes_1align_alloc_excess_used, 128, 1);
rt_realloc_naive!(rt_pow2_128bytes_1align_realloc_naive, 128, 1);
rt_realloc!(rt_pow2_128bytes_1align_realloc, 128, 1);
rt_realloc_excess_unused!(rt_pow2_128bytes_1align_realloc_excess_unused, 128, 1);
rt_realloc_excess_used!(rt_pow2_128bytes_1align_realloc_excess_used, 128, 1);

rt_calloc!(rt_pow2_256bytes_1align_calloc, 256, 1);
rt_mallocx!(rt_pow2_256bytes_1align_mallocx, 256, 1);
rt_mallocx_zeroed!(rt_pow2_256bytes_1align_mallocx_zeroed, 256, 1);
rt_mallocx_nallocx!(rt_pow2_256bytes_1align_mallocx_nallocx, 256, 1);
rt_alloc_layout_checked!(rt_pow2_256bytes_1align_alloc_layout_checked, 256, 1);
rt_alloc_layout_unchecked!(rt_pow2_256bytes_1align_alloc_layout_unchecked, 256, 1);
rt_alloc_excess_unused!(rt_pow2_256bytes_1align_alloc_excess_unused, 256, 1);
rt_alloc_excess_used!(rt_pow2_256bytes_1align_alloc_excess_used, 256, 1);
rt_realloc_naive!(rt_pow2_256bytes_1align_realloc_naive, 256, 1);
rt_realloc!(rt_pow2_256bytes_1align_realloc, 256, 1);
rt_realloc_excess_unused!(rt_pow2_256bytes_1align_realloc_excess_unused, 256, 1);
rt_realloc_excess_used!(rt_pow2_256bytes_1align_realloc_excess_used, 256, 1);

rt_calloc!(rt_pow2_512bytes_1align_calloc, 512, 1);
rt_mallocx!(rt_pow2_512bytes_1align_mallocx, 512, 1);
rt_mallocx_zeroed!(rt_pow2_512bytes_1align_mallocx_zeroed, 512, 1);
rt_mallocx_nallocx!(rt_pow2_512bytes_1align_mallocx_nallocx, 512, 1);
rt_alloc_layout_checked!(rt_pow2_512bytes_1align_alloc_layout_checked, 512, 1);
rt_alloc_layout_unchecked!(rt_pow2_512bytes_1align_alloc_layout_unchecked, 512, 1);
rt_alloc_excess_unused!(rt_pow2_512bytes_1align_alloc_excess_unused, 512, 1);
rt_alloc_excess_used!(rt_pow2_512bytes_1align_alloc_excess_used, 512, 1);
rt_realloc_naive!(rt_pow2_512bytes_1align_realloc_naive, 512, 1);
rt_realloc!(rt_pow2_512bytes_1align_realloc, 512, 1);
rt_realloc_excess_unused!(rt_pow2_512bytes_1align_realloc_excess_unused, 512, 1);
rt_realloc_excess_used!(rt_pow2_512bytes_1align_realloc_excess_used, 512, 1);

rt_calloc!(rt_pow2_1024bytes_1align_calloc, 1024, 1);
rt_mallocx!(rt_pow2_1024bytes_1align_mallocx, 1024, 1);
rt_mallocx_zeroed!(rt_pow2_1024bytes_1align_mallocx_zeroed, 1024, 1);
rt_mallocx_nallocx!(rt_pow2_1024bytes_1align_mallocx_nallocx, 1024, 1);
rt_alloc_layout_checked!(rt_pow2_1024bytes_1align_alloc_layout_checked, 1024, 1);
rt_alloc_layout_unchecked!(rt_pow2_1024bytes_1align_alloc_layout_unchecked, 1024, 1);
rt_alloc_excess_unused!(rt_pow2_1024bytes_1align_alloc_excess_unused, 1024, 1);
rt_alloc_excess_used!(rt_pow2_1024bytes_1align_alloc_excess_used, 1024, 1);
rt_realloc_naive!(rt_pow2_1024bytes_1align_realloc_naive, 1024, 1);
rt_realloc!(rt_pow2_1024bytes_1align_realloc, 1024, 1);
rt_realloc_excess_unused!(rt_pow2_1024bytes_1align_realloc_excess_unused, 1024, 1);
rt_realloc_excess_used!(rt_pow2_1024bytes_1align_realloc_excess_used, 1024, 1);

rt_calloc!(rt_pow2_2048bytes_1align_calloc, 2048, 1);
rt_mallocx!(rt_pow2_2048bytes_1align_mallocx, 2048, 1);
rt_mallocx_zeroed!(rt_pow2_2048bytes_1align_mallocx_zeroed, 2048, 1);
rt_mallocx_nallocx!(rt_pow2_2048bytes_1align_mallocx_nallocx, 2048, 1);
rt_alloc_layout_checked!(rt_pow2_2048bytes_1align_alloc_layout_checked, 2048, 1);
rt_alloc_layout_unchecked!(rt_pow2_2048bytes_1align_alloc_layout_unchecked, 2048, 1);
rt_alloc_excess_unused!(rt_pow2_2048bytes_1align_alloc_excess_unused, 2048, 1);
rt_alloc_excess_used!(rt_pow2_2048bytes_1align_alloc_excess_used, 2048, 1);
rt_realloc_naive!(rt_pow2_2048bytes_1align_realloc_naive, 2048, 1);
rt_realloc!(rt_pow2_2048bytes_1align_realloc, 2048, 1);
rt_realloc_excess_unused!(rt_pow2_2048bytes_1align_realloc_excess_unused, 2048, 1);
rt_realloc_excess_used!(rt_pow2_2048bytes_1align_realloc_excess_used, 2048, 1);

rt_calloc!(rt_pow2_4096bytes_1align_calloc, 4096, 1);
rt_mallocx!(rt_pow2_4096bytes_1align_mallocx, 4096, 1);
rt_mallocx_zeroed!(rt_pow2_4096bytes_1align_mallocx_zeroed, 4096, 1);
rt_mallocx_nallocx!(rt_pow2_4096bytes_1align_mallocx_nallocx, 4096, 1);
rt_alloc_layout_checked!(rt_pow2_4096bytes_1align_alloc_layout_checked, 4096, 1);
rt_alloc_layout_unchecked!(rt_pow2_4096bytes_1align_alloc_layout_unchecked, 4096, 1);
rt_alloc_excess_unused!(rt_pow2_4096bytes_1align_alloc_excess_unused, 4096, 1);
rt_alloc_excess_used!(rt_pow2_4096bytes_1align_alloc_excess_used, 4096, 1);
rt_realloc_naive!(rt_pow2_4096bytes_1align_realloc_naive, 4096, 1);
rt_realloc!(rt_pow2_4096bytes_1align_realloc, 4096, 1);
rt_realloc_excess_unused!(rt_pow2_4096bytes_1align_realloc_excess_unused, 4096, 1);
rt_realloc_excess_used!(rt_pow2_4096bytes_1align_realloc_excess_used, 4096, 1);

rt_calloc!(rt_pow2_8192bytes_1align_calloc, 8192, 1);
rt_mallocx!(rt_pow2_8192bytes_1align_mallocx, 8192, 1);
rt_mallocx_zeroed!(rt_pow2_8192bytes_1align_mallocx_zeroed, 8192, 1);
rt_mallocx_nallocx!(rt_pow2_8192bytes_1align_mallocx_nallocx, 8192, 1);
rt_alloc_layout_checked!(rt_pow2_8192bytes_1align_alloc_layout_checked, 8192, 1);
rt_alloc_layout_unchecked!(rt_pow2_8192bytes_1align_alloc_layout_unchecked, 8192, 1);
rt_alloc_excess_unused!(rt_pow2_8192bytes_1align_alloc_excess_unused, 8192, 1);
rt_alloc_excess_used!(rt_pow2_8192bytes_1align_alloc_excess_used, 8192, 1);
rt_realloc_naive!(rt_pow2_8192bytes_1align_realloc_naive, 8192, 1);
rt_realloc!(rt_pow2_8192bytes_1align_realloc, 8192, 1);
rt_realloc_excess_unused!(rt_pow2_8192bytes_1align_realloc_excess_unused, 8192, 1);
rt_realloc_excess_used!(rt_pow2_8192bytes_1align_realloc_excess_used, 8192, 1);

rt_calloc!(rt_pow2_16384bytes_1align_calloc, 16384, 1);
rt_mallocx!(rt_pow2_16384bytes_1align_mallocx, 16384, 1);
rt_mallocx_zeroed!(rt_pow2_16384bytes_1align_mallocx_zeroed, 16384, 1);
rt_mallocx_nallocx!(rt_pow2_16384bytes_1align_mallocx_nallocx, 16384, 1);
rt_alloc_layout_checked!(rt_pow2_16384bytes_1align_alloc_layout_checked, 16384, 1);
rt_alloc_layout_unchecked!(rt_pow2_16384bytes_1align_alloc_layout_unchecked, 16384, 1);
rt_alloc_excess_unused!(rt_pow2_16384bytes_1align_alloc_excess_unused, 16384, 1);
rt_alloc_excess_used!(rt_pow2_16384bytes_1align_alloc_excess_used, 16384, 1);
rt_realloc_naive!(rt_pow2_16384bytes_1align_realloc_naive, 16384, 1);
rt_realloc!(rt_pow2_16384bytes_1align_realloc, 16384, 1);
rt_realloc_excess_unused!(rt_pow2_16384bytes_1align_realloc_excess_unused, 16384, 1);
rt_realloc_excess_used!(rt_pow2_16384bytes_1align_realloc_excess_used, 16384, 1);

rt_calloc!(rt_pow2_32768bytes_1align_calloc, 32768, 1);
rt_mallocx!(rt_pow2_32768bytes_1align_mallocx, 32768, 1);
rt_mallocx_zeroed!(rt_pow2_32768bytes_1align_mallocx_zeroed, 32768, 1);
rt_mallocx_nallocx!(rt_pow2_32768bytes_1align_mallocx_nallocx, 32768, 1);
rt_alloc_layout_checked!(rt_pow2_32768bytes_1align_alloc_layout_checked, 32768, 1);
rt_alloc_layout_unchecked!(rt_pow2_32768bytes_1align_alloc_layout_unchecked, 32768, 1);
rt_alloc_excess_unused!(rt_pow2_32768bytes_1align_alloc_excess_unused, 32768, 1);
rt_alloc_excess_used!(rt_pow2_32768bytes_1align_alloc_excess_used, 32768, 1);
rt_realloc_naive!(rt_pow2_32768bytes_1align_realloc_naive, 32768, 1);
rt_realloc!(rt_pow2_32768bytes_1align_realloc, 32768, 1);
rt_realloc_excess_unused!(rt_pow2_32768bytes_1align_realloc_excess_unused, 32768, 1);
rt_realloc_excess_used!(rt_pow2_32768bytes_1align_realloc_excess_used, 32768, 1);

rt_calloc!(rt_pow2_65536bytes_1align_calloc, 65536, 1);
rt_mallocx!(rt_pow2_65536bytes_1align_mallocx, 65536, 1);
rt_mallocx_zeroed!(rt_pow2_65536bytes_1align_mallocx_zeroed, 65536, 1);
rt_mallocx_nallocx!(rt_pow2_65536bytes_1align_mallocx_nallocx, 65536, 1);
rt_alloc_layout_checked!(rt_pow2_65536bytes_1align_alloc_layout_checked, 65536, 1);
rt_alloc_layout_unchecked!(rt_pow2_65536bytes_1align_alloc_layout_unchecked, 65536, 1);
rt_alloc_excess_unused!(rt_pow2_65536bytes_1align_alloc_excess_unused, 65536, 1);
rt_alloc_excess_used!(rt_pow2_65536bytes_1align_alloc_excess_used, 65536, 1);
rt_realloc_naive!(rt_pow2_65536bytes_1align_realloc_naive, 65536, 1);
rt_realloc!(rt_pow2_65536bytes_1align_realloc, 65536, 1);
rt_realloc_excess_unused!(rt_pow2_65536bytes_1align_realloc_excess_unused, 65536, 1);
rt_realloc_excess_used!(rt_pow2_65536bytes_1align_realloc_excess_used, 65536, 1);

rt_calloc!(rt_pow2_131072bytes_1align_calloc, 131072, 1);
rt_mallocx!(rt_pow2_131072bytes_1align_mallocx, 131072, 1);
rt_mallocx_zeroed!(rt_pow2_131072bytes_1align_mallocx_zeroed, 131072, 1);
rt_mallocx_nallocx!(rt_pow2_131072bytes_1align_mallocx_nallocx, 131072, 1);
rt_alloc_layout_checked!(rt_pow2_131072bytes_1align_alloc_layout_checked, 131072, 1);
rt_alloc_layout_unchecked!(rt_pow2_131072bytes_1align_alloc_layout_unchecked, 131072, 1);
rt_alloc_excess_unused!(rt_pow2_131072bytes_1align_alloc_excess_unused, 131072, 1);
rt_alloc_excess_used!(rt_pow2_131072bytes_1align_alloc_excess_used, 131072, 1);
rt_realloc_naive!(rt_pow2_131072bytes_1align_realloc_naive, 131072, 1);
rt_realloc!(rt_pow2_131072bytes_1align_realloc, 131072, 1);
rt_realloc_excess_unused!(rt_pow2_131072bytes_1align_realloc_excess_unused, 131072, 1);
rt_realloc_excess_used!(rt_pow2_131072bytes_1align_realloc_excess_used, 131072, 1);

rt_calloc!(rt_pow2_4194304bytes_1align_calloc, 4194304, 1);
rt_mallocx!(rt_pow2_4194304bytes_1align_mallocx, 4194304, 1);
rt_mallocx_zeroed!(rt_pow2_4194304bytes_1align_mallocx_zeroed, 4194304, 1);
rt_mallocx_nallocx!(rt_pow2_4194304bytes_1align_mallocx_nallocx, 4194304, 1);
rt_alloc_layout_checked!(rt_pow2_4194304bytes_1align_alloc_layout_checked, 4194304, 1);
rt_alloc_layout_unchecked!(
    rt_pow2_4194304bytes_1align_alloc_layout_unchecked,
    4194304,
    1
);
rt_alloc_excess_unused!(rt_pow2_4194304bytes_1align_alloc_excess_unused, 4194304, 1);
rt_alloc_excess_used!(rt_pow2_4194304bytes_1align_alloc_excess_used, 4194304, 1);
rt_realloc_naive!(rt_pow2_4194304bytes_1align_realloc_naive, 4194304, 1);
rt_realloc!(rt_pow2_4194304bytes_1align_realloc, 4194304, 1);
rt_realloc_excess_unused!(
    rt_pow2_4194304bytes_1align_realloc_excess_unused,
    4194304,
    1
);
rt_realloc_excess_used!(rt_pow2_4194304bytes_1align_realloc_excess_used, 4194304, 1);

// Even
rt_calloc!(rt_even_10bytes_1align_calloc, 10, 1);
rt_mallocx!(rt_even_10bytes_1align_mallocx, 10, 1);
rt_mallocx_zeroed!(rt_even_10bytes_1align_mallocx_zeroed, 10, 1);
rt_mallocx_nallocx!(rt_even_10bytes_1align_mallocx_nallocx, 10, 1);
rt_alloc_layout_checked!(rt_even_10bytes_1align_alloc_layout_checked, 10, 1);
rt_alloc_layout_unchecked!(rt_even_10bytes_1align_alloc_layout_unchecked, 10, 1);
rt_alloc_excess_unused!(rt_even_10bytes_1align_alloc_excess_unused, 10, 1);
rt_alloc_excess_used!(rt_even_10bytes_1align_alloc_excess_used, 10, 1);
rt_realloc_naive!(rt_even_10bytes_1align_realloc_naive, 10, 1);
rt_realloc!(rt_even_10bytes_1align_realloc, 10, 1);
rt_realloc_excess_unused!(rt_even_10bytes_1align_realloc_excess_unused, 10, 1);
rt_realloc_excess_used!(rt_even_10bytes_1align_realloc_excess_used, 10, 1);

rt_calloc!(rt_even_100bytes_1align_calloc, 100, 1);
rt_mallocx!(rt_even_100bytes_1align_mallocx, 100, 1);
rt_mallocx_zeroed!(rt_even_100bytes_1align_mallocx_zeroed, 100, 1);
rt_mallocx_nallocx!(rt_even_100bytes_1align_mallocx_nallocx, 100, 1);
rt_alloc_layout_checked!(rt_even_100bytes_1align_alloc_layout_checked, 100, 1);
rt_alloc_layout_unchecked!(rt_even_100bytes_1align_alloc_layout_unchecked, 100, 1);
rt_alloc_excess_unused!(rt_even_100bytes_1align_alloc_excess_unused, 100, 1);
rt_alloc_excess_used!(rt_even_100bytes_1align_alloc_excess_used, 100, 1);
rt_realloc_naive!(rt_even_100bytes_1align_realloc_naive, 100, 1);
rt_realloc!(rt_even_100bytes_1align_realloc, 100, 1);
rt_realloc_excess_unused!(rt_even_100bytes_1align_realloc_excess_unused, 100, 1);
rt_realloc_excess_used!(rt_even_100bytes_1align_realloc_excess_used, 100, 1);

rt_calloc!(rt_even_1000bytes_1align_calloc, 1000, 1);
rt_mallocx!(rt_even_1000bytes_1align_mallocx, 1000, 1);
rt_mallocx_zeroed!(rt_even_1000bytes_1align_mallocx_zeroed, 1000, 1);
rt_mallocx_nallocx!(rt_even_1000bytes_1align_mallocx_nallocx, 1000, 1);
rt_alloc_layout_checked!(rt_even_1000bytes_1align_alloc_layout_checked, 1000, 1);
rt_alloc_layout_unchecked!(rt_even_1000bytes_1align_alloc_layout_unchecked, 1000, 1);
rt_alloc_excess_unused!(rt_even_1000bytes_1align_alloc_excess_unused, 1000, 1);
rt_alloc_excess_used!(rt_even_1000bytes_1align_alloc_excess_used, 1000, 1);
rt_realloc_naive!(rt_even_1000bytes_1align_realloc_naive, 1000, 1);
rt_realloc!(rt_even_1000bytes_1align_realloc, 1000, 1);
rt_realloc_excess_unused!(rt_even_1000bytes_1align_realloc_excess_unused, 1000, 1);
rt_realloc_excess_used!(rt_even_1000bytes_1align_realloc_excess_used, 1000, 1);

rt_calloc!(rt_even_10000bytes_1align_calloc, 10000, 1);
rt_mallocx!(rt_even_10000bytes_1align_mallocx, 10000, 1);
rt_mallocx_zeroed!(rt_even_10000bytes_1align_mallocx_zeroed, 10000, 1);
rt_mallocx_nallocx!(rt_even_10000bytes_1align_mallocx_nallocx, 10000, 1);
rt_alloc_layout_checked!(rt_even_10000bytes_1align_alloc_layout_checked, 10000, 1);
rt_alloc_layout_unchecked!(rt_even_10000bytes_1align_alloc_layout_unchecked, 10000, 1);
rt_alloc_excess_unused!(rt_even_10000bytes_1align_alloc_excess_unused, 10000, 1);
rt_alloc_excess_used!(rt_even_10000bytes_1align_alloc_excess_used, 10000, 1);
rt_realloc_naive!(rt_even_10000bytes_1align_realloc_naive, 10000, 1);
rt_realloc!(rt_even_10000bytes_1align_realloc, 10000, 1);
rt_realloc_excess_unused!(rt_even_10000bytes_1align_realloc_excess_unused, 10000, 1);
rt_realloc_excess_used!(rt_even_10000bytes_1align_realloc_excess_used, 10000, 1);

rt_calloc!(rt_even_100000bytes_1align_calloc, 100000, 1);
rt_mallocx!(rt_even_100000bytes_1align_mallocx, 100000, 1);
rt_mallocx_zeroed!(rt_even_100000bytes_1align_mallocx_zeroed, 100000, 1);
rt_mallocx_nallocx!(rt_even_100000bytes_1align_mallocx_nallocx, 100000, 1);
rt_alloc_layout_checked!(rt_even_100000bytes_1align_alloc_layout_checked, 100000, 1);
rt_alloc_layout_unchecked!(rt_even_100000bytes_1align_alloc_layout_unchecked, 100000, 1);
rt_alloc_excess_unused!(rt_even_100000bytes_1align_alloc_excess_unused, 100000, 1);
rt_alloc_excess_used!(rt_even_100000bytes_1align_alloc_excess_used, 100000, 1);
rt_realloc_naive!(rt_even_100000bytes_1align_realloc_naive, 100000, 1);
rt_realloc!(rt_even_100000bytes_1align_realloc, 100000, 1);
rt_realloc_excess_unused!(rt_even_100000bytes_1align_realloc_excess_unused, 100000, 1);
rt_realloc_excess_used!(rt_even_100000bytes_1align_realloc_excess_used, 100000, 1);

rt_calloc!(rt_even_1000000bytes_1align_calloc, 1000000, 1);
rt_mallocx!(rt_even_1000000bytes_1align_mallocx, 1000000, 1);
rt_mallocx_zeroed!(rt_even_1000000bytes_1align_mallocx_zeroed, 1000000, 1);
rt_mallocx_nallocx!(rt_even_1000000bytes_1align_mallocx_nallocx, 1000000, 1);
rt_alloc_layout_checked!(rt_even_1000000bytes_1align_alloc_layout_checked, 1000000, 1);
rt_alloc_layout_unchecked!(
    rt_even_1000000bytes_1align_alloc_layout_unchecked,
    1000000,
    1
);
rt_alloc_excess_unused!(rt_even_1000000bytes_1align_alloc_excess_unused, 1000000, 1);
rt_alloc_excess_used!(rt_even_1000000bytes_1align_alloc_excess_used, 1000000, 1);
rt_realloc_naive!(rt_even_1000000bytes_1align_realloc_naive, 1000000, 1);
rt_realloc!(rt_even_1000000bytes_1align_realloc, 1000000, 1);
rt_realloc_excess_unused!(
    rt_even_1000000bytes_1align_realloc_excess_unused,
    1000000,
    1
);
rt_realloc_excess_used!(rt_even_1000000bytes_1align_realloc_excess_used, 1000000, 1);

// Odd:
rt_calloc!(rt_odd_10bytes_1align_calloc, 10 - 1, 1);
rt_mallocx!(rt_odd_10bytes_1align_mallocx, 10 - 1, 1);
rt_mallocx_zeroed!(rt_odd_10bytes_1align_mallocx_zeroed, 10 - 1, 1);
rt_mallocx_nallocx!(rt_odd_10bytes_1align_mallocx_nallocx, 10 - 1, 1);
rt_alloc_layout_checked!(rt_odd_10bytes_1align_alloc_layout_checked, 10 - 1, 1);
rt_alloc_layout_unchecked!(rt_odd_10bytes_1align_alloc_layout_unchecked, 10 - 1, 1);
rt_alloc_excess_unused!(rt_odd_10bytes_1align_alloc_excess_unused, 10 - 1, 1);
rt_alloc_excess_used!(rt_odd_10bytes_1align_alloc_excess_used, 10 - 1, 1);
rt_realloc_naive!(rt_odd_10bytes_1align_realloc_naive, 10 - 1, 1);
rt_realloc!(rt_odd_10bytes_1align_realloc, 10 - 1, 1);
rt_realloc_excess_unused!(rt_odd_10bytes_1align_realloc_excess_unused, 10 - 1, 1);
rt_realloc_excess_used!(rt_odd_10bytes_1align_realloc_excess_used, 10 - 1, 1);

rt_calloc!(rt_odd_100bytes_1align_calloc, 100 - 1, 1);
rt_mallocx!(rt_odd_100bytes_1align_mallocx, 100 - 1, 1);
rt_mallocx_zeroed!(rt_odd_100bytes_1align_mallocx_zeroed, 100 - 1, 1);
rt_mallocx_nallocx!(rt_odd_100bytes_1align_mallocx_nallocx, 100 - 1, 1);
rt_alloc_layout_checked!(rt_odd_100bytes_1align_alloc_layout_checked, 100 - 1, 1);
rt_alloc_layout_unchecked!(rt_odd_100bytes_1align_alloc_layout_unchecked, 100 - 1, 1);
rt_alloc_excess_unused!(rt_odd_100bytes_1align_alloc_excess_unused, 100 - 1, 1);
rt_alloc_excess_used!(rt_odd_100bytes_1align_alloc_excess_used, 100 - 1, 1);
rt_realloc_naive!(rt_odd_100bytes_1align_realloc_naive, 100 - 1, 1);
rt_realloc!(rt_odd_100bytes_1align_realloc, 100 - 1, 1);
rt_realloc_excess_unused!(rt_odd_100bytes_1align_realloc_excess_unused, 100 - 1, 1);
rt_realloc_excess_used!(rt_odd_100bytes_1align_realloc_excess_used, 100 - 1, 1);

rt_calloc!(rt_odd_1000bytes_1align_calloc, 1000 - 1, 1);
rt_mallocx!(rt_odd_1000bytes_1align_mallocx, 1000 - 1, 1);
rt_mallocx_zeroed!(rt_odd_1000bytes_1align_mallocx_zeroed, 1000 - 1, 1);
rt_mallocx_nallocx!(rt_odd_1000bytes_1align_mallocx_nallocx, 1000 - 1, 1);
rt_alloc_layout_checked!(rt_odd_1000bytes_1align_alloc_layout_checked, 1000 - 1, 1);
rt_alloc_layout_unchecked!(rt_odd_1000bytes_1align_alloc_layout_unchecked, 1000 - 1, 1);
rt_alloc_excess_unused!(rt_odd_1000bytes_1align_alloc_excess_unused, 1000 - 1, 1);
rt_alloc_excess_used!(rt_odd_1000bytes_1align_alloc_excess_used, 1000 - 1, 1);
rt_realloc_naive!(rt_odd_1000bytes_1align_realloc_naive, 1000 - 1, 1);
rt_realloc!(rt_odd_1000bytes_1align_realloc, 1000 - 1, 1);
rt_realloc_excess_unused!(rt_odd_1000bytes_1align_realloc_excess_unused, 1000 - 1, 1);
rt_realloc_excess_used!(rt_odd_1000bytes_1align_realloc_excess_used, 1000 - 1, 1);

rt_calloc!(rt_odd_10000bytes_1align_calloc, 10000 - 1, 1);
rt_mallocx!(rt_odd_10000bytes_1align_mallocx, 10000 - 1, 1);
rt_mallocx_zeroed!(rt_odd_10000bytes_1align_mallocx_zeroed, 10000 - 1, 1);
rt_mallocx_nallocx!(rt_odd_10000bytes_1align_mallocx_nallocx, 10000 - 1, 1);
rt_alloc_layout_checked!(rt_odd_10000bytes_1align_alloc_layout_checked, 10000 - 1, 1);
rt_alloc_layout_unchecked!(
    rt_odd_10000bytes_1align_alloc_layout_unchecked,
    10000 - 1,
    1
);
rt_alloc_excess_unused!(rt_odd_10000bytes_1align_alloc_excess_unused, 10000 - 1, 1);
rt_alloc_excess_used!(rt_odd_10000bytes_1align_alloc_excess_used, 10000 - 1, 1);
rt_realloc_naive!(rt_odd_10000bytes_1align_realloc_naive, 10000 - 1, 1);
rt_realloc!(rt_odd_10000bytes_1align_realloc, 10000 - 1, 1);
rt_realloc_excess_unused!(rt_odd_10000bytes_1align_realloc_excess_unused, 10000 - 1, 1);
rt_realloc_excess_used!(rt_odd_10000bytes_1align_realloc_excess_used, 10000 - 1, 1);

rt_calloc!(rt_odd_100000bytes_1align_calloc, 100000 - 1, 1);
rt_mallocx!(rt_odd_100000bytes_1align_mallocx, 100000 - 1, 1);
rt_mallocx_zeroed!(rt_odd_100000bytes_1align_mallocx_zeroed, 100000 - 1, 1);
rt_mallocx_nallocx!(rt_odd_100000bytes_1align_mallocx_nallocx, 100000 - 1, 1);
rt_alloc_layout_checked!(
    rt_odd_100000bytes_1align_alloc_layout_checked,
    100000 - 1,
    1
);
rt_alloc_layout_unchecked!(
    rt_odd_100000bytes_1align_alloc_layout_unchecked,
    100000 - 1,
    1
);
rt_alloc_excess_unused!(rt_odd_100000bytes_1align_alloc_excess_unused, 100000 - 1, 1);
rt_alloc_excess_used!(rt_odd_100000bytes_1align_alloc_excess_used, 100000 - 1, 1);
rt_realloc_naive!(rt_odd_100000bytes_1align_realloc_naive, 100000 - 1, 1);
rt_realloc!(rt_odd_100000bytes_1align_realloc, 100000 - 1, 1);
rt_realloc_excess_unused!(
    rt_odd_100000bytes_1align_realloc_excess_unused,
    100000 - 1,
    1
);
rt_realloc_excess_used!(rt_odd_100000bytes_1align_realloc_excess_used, 100000 - 1, 1);

rt_calloc!(rt_odd_1000000bytes_1align_calloc, 1000000 - 1, 1);
rt_mallocx!(rt_odd_1000000bytes_1align_mallocx, 1000000 - 1, 1);
rt_mallocx_zeroed!(rt_odd_1000000bytes_1align_mallocx_zeroed, 1000000 - 1, 1);
rt_mallocx_nallocx!(rt_odd_1000000bytes_1align_mallocx_nallocx, 1000000 - 1, 1);
rt_alloc_layout_checked!(
    rt_odd_1000000bytes_1align_alloc_layout_checked,
    1000000 - 1,
    1
);
rt_alloc_layout_unchecked!(
    rt_odd_1000000bytes_1align_alloc_layout_unchecked,
    1000000 - 1,
    1
);
rt_alloc_excess_unused!(
    rt_odd_1000000bytes_1align_alloc_excess_unused,
    1000000 - 1,
    1
);
rt_alloc_excess_used!(rt_odd_1000000bytes_1align_alloc_excess_used, 1000000 - 1, 1);
rt_realloc_naive!(rt_odd_1000000bytes_1align_realloc_naive, 1000000 - 1, 1);
rt_realloc!(rt_odd_1000000bytes_1align_realloc, 1000000 - 1, 1);
rt_realloc_excess_unused!(
    rt_odd_1000000bytes_1align_realloc_excess_unused,
    1000000 - 1,
    1
);
rt_realloc_excess_used!(
    rt_odd_1000000bytes_1align_realloc_excess_used,
    1000000 - 1,
    1
);

// primes
rt_calloc!(rt_primes_3bytes_1align_calloc, 3, 1);
rt_mallocx!(rt_primes_3bytes_1align_mallocx, 3, 1);
rt_mallocx_zeroed!(rt_primes_3bytes_1align_mallocx_zeroed, 3, 1);
rt_mallocx_nallocx!(rt_primes_3bytes_1align_mallocx_nallocx, 3, 1);
rt_alloc_layout_checked!(rt_primes_3bytes_1align_alloc_layout_checked, 3, 1);
rt_alloc_layout_unchecked!(rt_primes_3bytes_1align_alloc_layout_unchecked, 3, 1);
rt_alloc_excess_unused!(rt_primes_3bytes_1align_alloc_excess_unused, 3, 1);
rt_alloc_excess_used!(rt_primes_3bytes_1align_alloc_excess_used, 3, 1);
rt_realloc_naive!(rt_primes_3bytes_1align_realloc_naive, 3, 1);
rt_realloc!(rt_primes_3bytes_1align_realloc, 3, 1);
rt_realloc_excess_unused!(rt_primes_3bytes_1align_realloc_excess_unused, 3, 1);
rt_realloc_excess_used!(rt_primes_3bytes_1align_realloc_excess_used, 3, 1);

rt_calloc!(rt_primes_7bytes_1align_calloc, 7, 1);
rt_mallocx!(rt_primes_7bytes_1align_mallocx, 7, 1);
rt_mallocx_zeroed!(rt_primes_7bytes_1align_mallocx_zeroed, 7, 1);
rt_mallocx_nallocx!(rt_primes_7bytes_1align_mallocx_nallocx, 7, 1);
rt_alloc_layout_checked!(rt_primes_7bytes_1align_alloc_layout_checked, 7, 1);
rt_alloc_layout_unchecked!(rt_primes_7bytes_1align_alloc_layout_unchecked, 7, 1);
rt_alloc_excess_unused!(rt_primes_7bytes_1align_alloc_excess_unused, 7, 1);
rt_alloc_excess_used!(rt_primes_7bytes_1align_alloc_excess_used, 7, 1);
rt_realloc_naive!(rt_primes_7bytes_1align_realloc_naive, 7, 1);
rt_realloc!(rt_primes_7bytes_1align_realloc, 7, 1);
rt_realloc_excess_unused!(rt_primes_7bytes_1align_realloc_excess_unused, 7, 1);
rt_realloc_excess_used!(rt_primes_7bytes_1align_realloc_excess_used, 7, 1);

rt_calloc!(rt_primes_13bytes_1align_calloc, 13, 1);
rt_mallocx!(rt_primes_13bytes_1align_mallocx, 13, 1);
rt_mallocx_zeroed!(rt_primes_13bytes_1align_mallocx_zeroed, 13, 1);
rt_mallocx_nallocx!(rt_primes_13bytes_1align_mallocx_nallocx, 13, 1);
rt_alloc_layout_checked!(rt_primes_13bytes_1align_alloc_layout_checked, 13, 1);
rt_alloc_layout_unchecked!(rt_primes_13bytes_1align_alloc_layout_unchecked, 13, 1);
rt_alloc_excess_unused!(rt_primes_13bytes_1align_alloc_excess_unused, 13, 1);
rt_alloc_excess_used!(rt_primes_13bytes_1align_alloc_excess_used, 13, 1);
rt_realloc_naive!(rt_primes_13bytes_1align_realloc_naive, 13, 1);
rt_realloc!(rt_primes_13bytes_1align_realloc, 13, 1);
rt_realloc_excess_unused!(rt_primes_13bytes_1align_realloc_excess_unused, 13, 1);
rt_realloc_excess_used!(rt_primes_13bytes_1align_realloc_excess_used, 13, 1);

rt_calloc!(rt_primes_17bytes_1align_calloc, 17, 1);
rt_mallocx!(rt_primes_17bytes_1align_mallocx, 17, 1);
rt_mallocx_zeroed!(rt_primes_17bytes_1align_mallocx_zeroed, 17, 1);
rt_mallocx_nallocx!(rt_primes_17bytes_1align_mallocx_nallocx, 17, 1);
rt_alloc_layout_checked!(rt_primes_17bytes_1align_alloc_layout_checked, 17, 1);
rt_alloc_layout_unchecked!(rt_primes_17bytes_1align_alloc_layout_unchecked, 17, 1);
rt_alloc_excess_unused!(rt_primes_17bytes_1align_alloc_excess_unused, 17, 1);
rt_alloc_excess_used!(rt_primes_17bytes_1align_alloc_excess_used, 17, 1);
rt_realloc_naive!(rt_primes_17bytes_1align_realloc_naive, 17, 1);
rt_realloc!(rt_primes_17bytes_1align_realloc, 17, 1);
rt_realloc_excess_unused!(rt_primes_17bytes_1align_realloc_excess_unused, 17, 1);
rt_realloc_excess_used!(rt_primes_17bytes_1align_realloc_excess_used, 17, 1);

rt_calloc!(rt_primes_31bytes_1align_calloc, 31, 1);
rt_mallocx!(rt_primes_31bytes_1align_mallocx, 31, 1);
rt_mallocx_zeroed!(rt_primes_31bytes_1align_mallocx_zeroed, 31, 1);
rt_mallocx_nallocx!(rt_primes_31bytes_1align_mallocx_nallocx, 31, 1);
rt_alloc_layout_checked!(rt_primes_31bytes_1align_alloc_layout_checked, 31, 1);
rt_alloc_layout_unchecked!(rt_primes_31bytes_1align_alloc_layout_unchecked, 31, 1);
rt_alloc_excess_unused!(rt_primes_31bytes_1align_alloc_excess_unused, 31, 1);
rt_alloc_excess_used!(rt_primes_31bytes_1align_alloc_excess_used, 31, 1);
rt_realloc_naive!(rt_primes_31bytes_1align_realloc_naive, 31, 1);
rt_realloc!(rt_primes_31bytes_1align_realloc, 31, 1);
rt_realloc_excess_unused!(rt_primes_31bytes_1align_realloc_excess_unused, 31, 1);
rt_realloc_excess_used!(rt_primes_31bytes_1align_realloc_excess_used, 31, 1);

rt_calloc!(rt_primes_61bytes_1align_calloc, 61, 1);
rt_mallocx!(rt_primes_61bytes_1align_mallocx, 61, 1);
rt_mallocx_zeroed!(rt_primes_61bytes_1align_mallocx_zeroed, 61, 1);
rt_mallocx_nallocx!(rt_primes_61bytes_1align_mallocx_nallocx, 61, 1);
rt_alloc_layout_checked!(rt_primes_61bytes_1align_alloc_layout_checked, 61, 1);
rt_alloc_layout_unchecked!(rt_primes_61bytes_1align_alloc_layout_unchecked, 61, 1);
rt_alloc_excess_unused!(rt_primes_61bytes_1align_alloc_excess_unused, 61, 1);
rt_alloc_excess_used!(rt_primes_61bytes_1align_alloc_excess_used, 61, 1);
rt_realloc_naive!(rt_primes_61bytes_1align_realloc_naive, 61, 1);
rt_realloc!(rt_primes_61bytes_1align_realloc, 61, 1);
rt_realloc_excess_unused!(rt_primes_61bytes_1align_realloc_excess_unused, 61, 1);
rt_realloc_excess_used!(rt_primes_61bytes_1align_realloc_excess_used, 61, 1);

rt_calloc!(rt_primes_96bytes_1align_calloc, 96, 1);
rt_mallocx!(rt_primes_96bytes_1align_mallocx, 96, 1);
rt_mallocx_zeroed!(rt_primes_96bytes_1align_mallocx_zeroed, 96, 1);
rt_mallocx_nallocx!(rt_primes_96bytes_1align_mallocx_nallocx, 96, 1);
rt_alloc_layout_checked!(rt_primes_96bytes_1align_alloc_layout_checked, 96, 1);
rt_alloc_layout_unchecked!(rt_primes_96bytes_1align_alloc_layout_unchecked, 96, 1);
rt_alloc_excess_unused!(rt_primes_96bytes_1align_alloc_excess_unused, 96, 1);
rt_alloc_excess_used!(rt_primes_96bytes_1align_alloc_excess_used, 96, 1);
rt_realloc_naive!(rt_primes_96bytes_1align_realloc_naive, 96, 1);
rt_realloc!(rt_primes_96bytes_1align_realloc, 96, 1);
rt_realloc_excess_unused!(rt_primes_96bytes_1align_realloc_excess_unused, 96, 1);
rt_realloc_excess_used!(rt_primes_96bytes_1align_realloc_excess_used, 96, 1);

rt_calloc!(rt_primes_127bytes_1align_calloc, 127, 1);
rt_mallocx!(rt_primes_127bytes_1align_mallocx, 127, 1);
rt_mallocx_zeroed!(rt_primes_127bytes_1align_mallocx_zeroed, 127, 1);
rt_mallocx_nallocx!(rt_primes_127bytes_1align_mallocx_nallocx, 127, 1);
rt_alloc_layout_checked!(rt_primes_127bytes_1align_alloc_layout_checked, 127, 1);
rt_alloc_layout_unchecked!(rt_primes_127bytes_1align_alloc_layout_unchecked, 127, 1);
rt_alloc_excess_unused!(rt_primes_127bytes_1align_alloc_excess_unused, 127, 1);
rt_alloc_excess_used!(rt_primes_127bytes_1align_alloc_excess_used, 127, 1);
rt_realloc_naive!(rt_primes_127bytes_1align_realloc_naive, 127, 1);
rt_realloc!(rt_primes_127bytes_1align_realloc, 127, 1);
rt_realloc_excess_unused!(rt_primes_127bytes_1align_realloc_excess_unused, 127, 1);
rt_realloc_excess_used!(rt_primes_127bytes_1align_realloc_excess_used, 127, 1);

rt_calloc!(rt_primes_257bytes_1align_calloc, 257, 1);
rt_mallocx!(rt_primes_257bytes_1align_mallocx, 257, 1);
rt_mallocx_zeroed!(rt_primes_257bytes_1align_mallocx_zeroed, 257, 1);
rt_mallocx_nallocx!(rt_primes_257bytes_1align_mallocx_nallocx, 257, 1);
rt_alloc_layout_checked!(rt_primes_257bytes_1align_alloc_layout_checked, 257, 1);
rt_alloc_layout_unchecked!(rt_primes_257bytes_1align_alloc_layout_unchecked, 257, 1);
rt_alloc_excess_unused!(rt_primes_257bytes_1align_alloc_excess_unused, 257, 1);
rt_alloc_excess_used!(rt_primes_257bytes_1align_alloc_excess_used, 257, 1);
rt_realloc_naive!(rt_primes_257bytes_1align_realloc_naive, 257, 1);
rt_realloc!(rt_primes_257bytes_1align_realloc, 257, 1);
rt_realloc_excess_unused!(rt_primes_257bytes_1align_realloc_excess_unused, 257, 1);
rt_realloc_excess_used!(rt_primes_257bytes_1align_realloc_excess_used, 257, 1);

rt_calloc!(rt_primes_509bytes_1align_calloc, 509, 1);
rt_mallocx!(rt_primes_509bytes_1align_mallocx, 509, 1);
rt_mallocx_zeroed!(rt_primes_509bytes_1align_mallocx_zeroed, 509, 1);
rt_mallocx_nallocx!(rt_primes_509bytes_1align_mallocx_nallocx, 509, 1);
rt_alloc_layout_checked!(rt_primes_509bytes_1align_alloc_layout_checked, 509, 1);
rt_alloc_layout_unchecked!(rt_primes_509bytes_1align_alloc_layout_unchecked, 509, 1);
rt_alloc_excess_unused!(rt_primes_509bytes_1align_alloc_excess_unused, 509, 1);
rt_alloc_excess_used!(rt_primes_509bytes_1align_alloc_excess_used, 509, 1);
rt_realloc_naive!(rt_primes_509bytes_1align_realloc_naive, 509, 1);
rt_realloc!(rt_primes_509bytes_1align_realloc, 509, 1);
rt_realloc_excess_unused!(rt_primes_509bytes_1align_realloc_excess_unused, 509, 1);
rt_realloc_excess_used!(rt_primes_509bytes_1align_realloc_excess_used, 509, 1);

rt_calloc!(rt_primes_1021bytes_1align_calloc, 1021, 1);
rt_mallocx!(rt_primes_1021bytes_1align_mallocx, 1021, 1);
rt_mallocx_zeroed!(rt_primes_1021bytes_1align_mallocx_zeroed, 1021, 1);
rt_mallocx_nallocx!(rt_primes_1021bytes_1align_mallocx_nallocx, 1021, 1);
rt_alloc_layout_checked!(rt_primes_1021bytes_1align_alloc_layout_checked, 1021, 1);
rt_alloc_layout_unchecked!(rt_primes_1021bytes_1align_alloc_layout_unchecked, 1021, 1);
rt_alloc_excess_unused!(rt_primes_1021bytes_1align_alloc_excess_unused, 1021, 1);
rt_alloc_excess_used!(rt_primes_1021bytes_1align_alloc_excess_used, 1021, 1);
rt_realloc_naive!(rt_primes_1021bytes_1align_realloc_naive, 1021, 1);
rt_realloc!(rt_primes_1021bytes_1align_realloc, 1021, 1);
rt_realloc_excess_unused!(rt_primes_1021bytes_1align_realloc_excess_unused, 1021, 1);
rt_realloc_excess_used!(rt_primes_1021bytes_1align_realloc_excess_used, 1021, 1);

rt_calloc!(rt_primes_2039bytes_1align_calloc, 2039, 1);
rt_mallocx!(rt_primes_2039bytes_1align_mallocx, 2039, 1);
rt_mallocx_zeroed!(rt_primes_2039bytes_1align_mallocx_zeroed, 2039, 1);
rt_mallocx_nallocx!(rt_primes_2039bytes_1align_mallocx_nallocx, 2039, 1);
rt_alloc_layout_checked!(rt_primes_2039bytes_1align_alloc_layout_checked, 2039, 1);
rt_alloc_layout_unchecked!(rt_primes_2039bytes_1align_alloc_layout_unchecked, 2039, 1);
rt_alloc_excess_unused!(rt_primes_2039bytes_1align_alloc_excess_unused, 2039, 1);
rt_alloc_excess_used!(rt_primes_2039bytes_1align_alloc_excess_used, 2039, 1);
rt_realloc_naive!(rt_primes_2039bytes_1align_realloc_naive, 2039, 1);
rt_realloc!(rt_primes_2039bytes_1align_realloc, 2039, 1);
rt_realloc_excess_unused!(rt_primes_2039bytes_1align_realloc_excess_unused, 2039, 1);
rt_realloc_excess_used!(rt_primes_2039bytes_1align_realloc_excess_used, 2039, 1);

rt_calloc!(rt_primes_4093bytes_1align_calloc, 4093, 1);
rt_mallocx!(rt_primes_4093bytes_1align_mallocx, 4093, 1);
rt_mallocx_zeroed!(rt_primes_4093bytes_1align_mallocx_zeroed, 4093, 1);
rt_mallocx_nallocx!(rt_primes_4093bytes_1align_mallocx_nallocx, 4093, 1);
rt_alloc_layout_checked!(rt_primes_4093bytes_1align_alloc_layout_checked, 4093, 1);
rt_alloc_layout_unchecked!(rt_primes_4093bytes_1align_alloc_layout_unchecked, 4093, 1);
rt_alloc_excess_unused!(rt_primes_4093bytes_1align_alloc_excess_unused, 4093, 1);
rt_alloc_excess_used!(rt_primes_4093bytes_1align_alloc_excess_used, 4093, 1);
rt_realloc_naive!(rt_primes_4093bytes_1align_realloc_naive, 4093, 1);
rt_realloc!(rt_primes_4093bytes_1align_realloc, 4093, 1);
rt_realloc_excess_unused!(rt_primes_4093bytes_1align_realloc_excess_unused, 4093, 1);
rt_realloc_excess_used!(rt_primes_4093bytes_1align_realloc_excess_used, 4093, 1);

rt_calloc!(rt_primes_8191bytes_1align_calloc, 8191, 1);
rt_mallocx!(rt_primes_8191bytes_1align_mallocx, 8191, 1);
rt_mallocx_zeroed!(rt_primes_8191bytes_1align_mallocx_zeroed, 8191, 1);
rt_mallocx_nallocx!(rt_primes_8191bytes_1align_mallocx_nallocx, 8191, 1);
rt_alloc_layout_checked!(rt_primes_8191bytes_1align_alloc_layout_checked, 8191, 1);
rt_alloc_layout_unchecked!(rt_primes_8191bytes_1align_alloc_layout_unchecked, 8191, 1);
rt_alloc_excess_unused!(rt_primes_8191bytes_1align_alloc_excess_unused, 8191, 1);
rt_alloc_excess_used!(rt_primes_8191bytes_1align_alloc_excess_used, 8191, 1);
rt_realloc_naive!(rt_primes_8191bytes_1align_realloc_naive, 8191, 1);
rt_realloc!(rt_primes_8191bytes_1align_realloc, 8191, 1);
rt_realloc_excess_unused!(rt_primes_8191bytes_1align_realloc_excess_unused, 8191, 1);
rt_realloc_excess_used!(rt_primes_8191bytes_1align_realloc_excess_used, 8191, 1);

rt_calloc!(rt_primes_16381bytes_1align_calloc, 16381, 1);
rt_mallocx!(rt_primes_16381bytes_1align_mallocx, 16381, 1);
rt_mallocx_zeroed!(rt_primes_16381bytes_1align_mallocx_zeroed, 16381, 1);
rt_mallocx_nallocx!(rt_primes_16381bytes_1align_mallocx_nallocx, 16381, 1);
rt_alloc_layout_checked!(rt_primes_16381bytes_1align_alloc_layout_checked, 16381, 1);
rt_alloc_layout_unchecked!(rt_primes_16381bytes_1align_alloc_layout_unchecked, 16381, 1);
rt_alloc_excess_unused!(rt_primes_16381bytes_1align_alloc_excess_unused, 16381, 1);
rt_alloc_excess_used!(rt_primes_16381bytes_1align_alloc_excess_used, 16381, 1);
rt_realloc_naive!(rt_primes_16381bytes_1align_realloc_naive, 16381, 1);
rt_realloc!(rt_primes_16381bytes_1align_realloc, 16381, 1);
rt_realloc_excess_unused!(rt_primes_16381bytes_1align_realloc_excess_unused, 16381, 1);
rt_realloc_excess_used!(rt_primes_16381bytes_1align_realloc_excess_used, 16381, 1);

rt_calloc!(rt_primes_32749bytes_1align_calloc, 32749, 1);
rt_mallocx!(rt_primes_32749bytes_1align_mallocx, 32749, 1);
rt_mallocx_zeroed!(rt_primes_32749bytes_1align_mallocx_zeroed, 32749, 1);
rt_mallocx_nallocx!(rt_primes_32749bytes_1align_mallocx_nallocx, 32749, 1);
rt_alloc_layout_checked!(rt_primes_32749bytes_1align_alloc_layout_checked, 32749, 1);
rt_alloc_layout_unchecked!(rt_primes_32749bytes_1align_alloc_layout_unchecked, 32749, 1);
rt_alloc_excess_unused!(rt_primes_32749bytes_1align_alloc_excess_unused, 32749, 1);
rt_alloc_excess_used!(rt_primes_32749bytes_1align_alloc_excess_used, 32749, 1);
rt_realloc_naive!(rt_primes_32749bytes_1align_realloc_naive, 32749, 1);
rt_realloc!(rt_primes_32749bytes_1align_realloc, 32749, 1);
rt_realloc_excess_unused!(rt_primes_32749bytes_1align_realloc_excess_unused, 32749, 1);
rt_realloc_excess_used!(rt_primes_32749bytes_1align_realloc_excess_used, 32749, 1);

rt_calloc!(rt_primes_65537bytes_1align_calloc, 65537, 1);
rt_mallocx!(rt_primes_65537bytes_1align_mallocx, 65537, 1);
rt_mallocx_zeroed!(rt_primes_65537bytes_1align_mallocx_zeroed, 65537, 1);
rt_mallocx_nallocx!(rt_primes_65537bytes_1align_mallocx_nallocx, 65537, 1);
rt_alloc_layout_checked!(rt_primes_65537bytes_1align_alloc_layout_checked, 65537, 1);
rt_alloc_layout_unchecked!(rt_primes_65537bytes_1align_alloc_layout_unchecked, 65537, 1);
rt_alloc_excess_unused!(rt_primes_65537bytes_1align_alloc_excess_unused, 65537, 1);
rt_alloc_excess_used!(rt_primes_65537bytes_1align_alloc_excess_used, 65537, 1);
rt_realloc_naive!(rt_primes_65537bytes_1align_realloc_naive, 65537, 1);
rt_realloc!(rt_primes_65537bytes_1align_realloc, 65537, 1);
rt_realloc_excess_unused!(rt_primes_65537bytes_1align_realloc_excess_unused, 65537, 1);
rt_realloc_excess_used!(rt_primes_65537bytes_1align_realloc_excess_used, 65537, 1);

rt_calloc!(rt_primes_131071bytes_1align_calloc, 131071, 1);
rt_mallocx!(rt_primes_131071bytes_1align_mallocx, 131071, 1);
rt_mallocx_zeroed!(rt_primes_131071bytes_1align_mallocx_zeroed, 131071, 1);
rt_mallocx_nallocx!(rt_primes_131071bytes_1align_mallocx_nallocx, 131071, 1);
rt_alloc_layout_checked!(rt_primes_131071bytes_1align_alloc_layout_checked, 131071, 1);
rt_alloc_layout_unchecked!(
    rt_primes_131071bytes_1align_alloc_layout_unchecked,
    131071,
    1
);
rt_alloc_excess_unused!(rt_primes_131071bytes_1align_alloc_excess_unused, 131071, 1);
rt_alloc_excess_used!(rt_primes_131071bytes_1align_alloc_excess_used, 131071, 1);
rt_realloc_naive!(rt_primes_131071bytes_1align_realloc_naive, 131071, 1);
rt_realloc!(rt_primes_131071bytes_1align_realloc, 131071, 1);
rt_realloc_excess_unused!(
    rt_primes_131071bytes_1align_realloc_excess_unused,
    131071,
    1
);
rt_realloc_excess_used!(rt_primes_131071bytes_1align_realloc_excess_used, 131071, 1);

rt_calloc!(rt_primes_4194301bytes_1align_calloc, 4194301, 1);
rt_mallocx!(rt_primes_4194301bytes_1align_mallocx, 4194301, 1);
rt_mallocx_zeroed!(rt_primes_4194301bytes_1align_mallocx_zeroed, 4194301, 1);
rt_mallocx_nallocx!(rt_primes_4194301bytes_1align_mallocx_nallocx, 4194301, 1);
rt_alloc_layout_checked!(
    rt_primes_4194301bytes_1align_alloc_layout_checked,
    4194301,
    1
);
rt_alloc_layout_unchecked!(
    rt_primes_4194301bytes_1align_alloc_layout_unchecked,
    4194301,
    1
);
rt_alloc_excess_unused!(
    rt_primes_4194301bytes_1align_alloc_excess_unused,
    4194301,
    1
);
rt_alloc_excess_used!(rt_primes_4194301bytes_1align_alloc_excess_used, 4194301, 1);
rt_realloc_naive!(rt_primes_4194301bytes_1align_realloc_naive, 4194301, 1);
rt_realloc!(rt_primes_4194301bytes_1align_realloc, 4194301, 1);
rt_realloc_excess_unused!(
    rt_primes_4194301bytes_1align_realloc_excess_unused,
    4194301,
    1
);
rt_realloc_excess_used!(
    rt_primes_4194301bytes_1align_realloc_excess_used,
    4194301,
    1
);

// 2 bytes alignment

// Powers of two:
rt_calloc!(rt_pow2_1bytes_2align_calloc, 1, 2);
rt_mallocx!(rt_pow2_1bytes_2align_mallocx, 1, 2);
rt_mallocx_zeroed!(rt_pow2_1bytes_2align_mallocx_zeroed, 1, 2);
rt_mallocx_nallocx!(rt_pow2_1bytes_2align_mallocx_nallocx, 1, 2);
rt_alloc_layout_checked!(rt_pow2_1bytes_2align_alloc_layout_checked, 1, 2);
rt_alloc_layout_unchecked!(rt_pow2_1bytes_2align_alloc_layout_unchecked, 1, 2);
rt_alloc_excess_unused!(rt_pow2_1bytes_2align_alloc_excess_unused, 1, 2);
rt_alloc_excess_used!(rt_pow2_1bytes_2align_alloc_excess_used, 1, 2);
rt_realloc_naive!(rt_pow2_1bytes_2align_realloc_naive, 1, 2);
rt_realloc!(rt_pow2_1bytes_2align_realloc, 1, 2);
rt_realloc_excess_unused!(rt_pow2_1bytes_2align_realloc_excess_unused, 1, 2);
rt_realloc_excess_used!(rt_pow2_1bytes_2align_realloc_excess_used, 1, 2);

rt_calloc!(rt_pow2_2bytes_2align_calloc, 2, 2);
rt_mallocx!(rt_pow2_2bytes_2align_mallocx, 2, 2);
rt_mallocx_zeroed!(rt_pow2_2bytes_2align_mallocx_zeroed, 2, 2);
rt_mallocx_nallocx!(rt_pow2_2bytes_2align_mallocx_nallocx, 2, 2);
rt_alloc_layout_checked!(rt_pow2_2bytes_2align_alloc_layout_checked, 2, 2);
rt_alloc_layout_unchecked!(rt_pow2_2bytes_2align_alloc_layout_unchecked, 2, 2);
rt_alloc_excess_unused!(rt_pow2_2bytes_2align_alloc_excess_unused, 2, 2);
rt_alloc_excess_used!(rt_pow2_2bytes_2align_alloc_excess_used, 2, 2);
rt_realloc_naive!(rt_pow2_2bytes_2align_realloc_naive, 2, 2);
rt_realloc!(rt_pow2_2bytes_2align_realloc, 2, 2);
rt_realloc_excess_unused!(rt_pow2_2bytes_2align_realloc_excess_unused, 2, 2);
rt_realloc_excess_used!(rt_pow2_2bytes_2align_realloc_excess_used, 2, 2);

rt_calloc!(rt_pow2_4bytes_2align_calloc, 4, 2);
rt_mallocx!(rt_pow2_4bytes_2align_mallocx, 4, 2);
rt_mallocx_zeroed!(rt_pow2_4bytes_2align_mallocx_zeroed, 4, 2);
rt_mallocx_nallocx!(rt_pow2_4bytes_2align_mallocx_nallocx, 4, 2);
rt_alloc_layout_checked!(rt_pow2_4bytes_2align_alloc_layout_checked, 4, 2);
rt_alloc_layout_unchecked!(rt_pow2_4bytes_2align_alloc_layout_unchecked, 4, 2);
rt_alloc_excess_unused!(rt_pow2_4bytes_2align_alloc_excess_unused, 4, 2);
rt_alloc_excess_used!(rt_pow2_4bytes_2align_alloc_excess_used, 4, 2);
rt_realloc_naive!(rt_pow2_4bytes_2align_realloc_naive, 4, 2);
rt_realloc!(rt_pow2_4bytes_2align_realloc, 4, 2);
rt_realloc_excess_unused!(rt_pow2_4bytes_2align_realloc_excess_unused, 4, 2);
rt_realloc_excess_used!(rt_pow2_4bytes_2align_realloc_excess_used, 4, 2);

rt_calloc!(rt_pow2_8bytes_2align_calloc, 8, 2);
rt_mallocx!(rt_pow2_8bytes_2align_mallocx, 8, 2);
rt_mallocx_zeroed!(rt_pow2_8bytes_2align_mallocx_zeroed, 8, 2);
rt_mallocx_nallocx!(rt_pow2_8bytes_2align_mallocx_nallocx, 8, 2);
rt_alloc_layout_checked!(rt_pow2_8bytes_2align_alloc_layout_checked, 8, 2);
rt_alloc_layout_unchecked!(rt_pow2_8bytes_2align_alloc_layout_unchecked, 8, 2);
rt_alloc_excess_unused!(rt_pow2_8bytes_2align_alloc_excess_unused, 8, 2);
rt_alloc_excess_used!(rt_pow2_8bytes_2align_alloc_excess_used, 8, 2);
rt_realloc_naive!(rt_pow2_8bytes_2align_realloc_naive, 8, 2);
rt_realloc!(rt_pow2_8bytes_2align_realloc, 8, 2);
rt_realloc_excess_unused!(rt_pow2_8bytes_2align_realloc_excess_unused, 8, 2);
rt_realloc_excess_used!(rt_pow2_8bytes_2align_realloc_excess_used, 8, 2);

rt_calloc!(rt_pow2_16bytes_2align_calloc, 16, 2);
rt_mallocx!(rt_pow2_16bytes_2align_mallocx, 16, 2);
rt_mallocx_zeroed!(rt_pow2_16bytes_2align_mallocx_zeroed, 16, 2);
rt_mallocx_nallocx!(rt_pow2_16bytes_2align_mallocx_nallocx, 16, 2);
rt_alloc_layout_checked!(rt_pow2_16bytes_2align_alloc_layout_checked, 16, 2);
rt_alloc_layout_unchecked!(rt_pow2_16bytes_2align_alloc_layout_unchecked, 16, 2);
rt_alloc_excess_unused!(rt_pow2_16bytes_2align_alloc_excess_unused, 16, 2);
rt_alloc_excess_used!(rt_pow2_16bytes_2align_alloc_excess_used, 16, 2);
rt_realloc_naive!(rt_pow2_16bytes_2align_realloc_naive, 16, 2);
rt_realloc!(rt_pow2_16bytes_2align_realloc, 16, 2);
rt_realloc_excess_unused!(rt_pow2_16bytes_2align_realloc_excess_unused, 16, 2);
rt_realloc_excess_used!(rt_pow2_16bytes_2align_realloc_excess_used, 16, 2);

rt_calloc!(rt_pow2_32bytes_2align_calloc, 32, 2);
rt_mallocx!(rt_pow2_32bytes_2align_mallocx, 32, 2);
rt_mallocx_zeroed!(rt_pow2_32bytes_2align_mallocx_zeroed, 32, 2);
rt_mallocx_nallocx!(rt_pow2_32bytes_2align_mallocx_nallocx, 32, 2);
rt_alloc_layout_checked!(rt_pow2_32bytes_2align_alloc_layout_checked, 32, 2);
rt_alloc_layout_unchecked!(rt_pow2_32bytes_2align_alloc_layout_unchecked, 32, 2);
rt_alloc_excess_unused!(rt_pow2_32bytes_2align_alloc_excess_unused, 32, 2);
rt_alloc_excess_used!(rt_pow2_32bytes_2align_alloc_excess_used, 32, 2);
rt_realloc_naive!(rt_pow2_32bytes_2align_realloc_naive, 32, 2);
rt_realloc!(rt_pow2_32bytes_2align_realloc, 32, 2);
rt_realloc_excess_unused!(rt_pow2_32bytes_2align_realloc_excess_unused, 32, 2);
rt_realloc_excess_used!(rt_pow2_32bytes_2align_realloc_excess_used, 32, 2);

rt_calloc!(rt_pow2_64bytes_2align_calloc, 64, 2);
rt_mallocx!(rt_pow2_64bytes_2align_mallocx, 64, 2);
rt_mallocx_zeroed!(rt_pow2_64bytes_2align_mallocx_zeroed, 64, 2);
rt_mallocx_nallocx!(rt_pow2_64bytes_2align_mallocx_nallocx, 64, 2);
rt_alloc_layout_checked!(rt_pow2_64bytes_2align_alloc_layout_checked, 64, 2);
rt_alloc_layout_unchecked!(rt_pow2_64bytes_2align_alloc_layout_unchecked, 64, 2);
rt_alloc_excess_unused!(rt_pow2_64bytes_2align_alloc_excess_unused, 64, 2);
rt_alloc_excess_used!(rt_pow2_64bytes_2align_alloc_excess_used, 64, 2);
rt_realloc_naive!(rt_pow2_64bytes_2align_realloc_naive, 64, 2);
rt_realloc!(rt_pow2_64bytes_2align_realloc, 64, 2);
rt_realloc_excess_unused!(rt_pow2_64bytes_2align_realloc_excess_unused, 64, 2);
rt_realloc_excess_used!(rt_pow2_64bytes_2align_realloc_excess_used, 64, 2);

rt_calloc!(rt_pow2_128bytes_2align_calloc, 128, 2);
rt_mallocx!(rt_pow2_128bytes_2align_mallocx, 128, 2);
rt_mallocx_zeroed!(rt_pow2_128bytes_2align_mallocx_zeroed, 128, 2);
rt_mallocx_nallocx!(rt_pow2_128bytes_2align_mallocx_nallocx, 128, 2);
rt_alloc_layout_checked!(rt_pow2_128bytes_2align_alloc_layout_checked, 128, 2);
rt_alloc_layout_unchecked!(rt_pow2_128bytes_2align_alloc_layout_unchecked, 128, 2);
rt_alloc_excess_unused!(rt_pow2_128bytes_2align_alloc_excess_unused, 128, 2);
rt_alloc_excess_used!(rt_pow2_128bytes_2align_alloc_excess_used, 128, 2);
rt_realloc_naive!(rt_pow2_128bytes_2align_realloc_naive, 128, 2);
rt_realloc!(rt_pow2_128bytes_2align_realloc, 128, 2);
rt_realloc_excess_unused!(rt_pow2_128bytes_2align_realloc_excess_unused, 128, 2);
rt_realloc_excess_used!(rt_pow2_128bytes_2align_realloc_excess_used, 128, 2);

rt_calloc!(rt_pow2_256bytes_2align_calloc, 256, 2);
rt_mallocx!(rt_pow2_256bytes_2align_mallocx, 256, 2);
rt_mallocx_zeroed!(rt_pow2_256bytes_2align_mallocx_zeroed, 256, 2);
rt_mallocx_nallocx!(rt_pow2_256bytes_2align_mallocx_nallocx, 256, 2);
rt_alloc_layout_checked!(rt_pow2_256bytes_2align_alloc_layout_checked, 256, 2);
rt_alloc_layout_unchecked!(rt_pow2_256bytes_2align_alloc_layout_unchecked, 256, 2);
rt_alloc_excess_unused!(rt_pow2_256bytes_2align_alloc_excess_unused, 256, 2);
rt_alloc_excess_used!(rt_pow2_256bytes_2align_alloc_excess_used, 256, 2);
rt_realloc_naive!(rt_pow2_256bytes_2align_realloc_naive, 256, 2);
rt_realloc!(rt_pow2_256bytes_2align_realloc, 256, 2);
rt_realloc_excess_unused!(rt_pow2_256bytes_2align_realloc_excess_unused, 256, 2);
rt_realloc_excess_used!(rt_pow2_256bytes_2align_realloc_excess_used, 256, 2);

rt_calloc!(rt_pow2_512bytes_2align_calloc, 512, 2);
rt_mallocx!(rt_pow2_512bytes_2align_mallocx, 512, 2);
rt_mallocx_zeroed!(rt_pow2_512bytes_2align_mallocx_zeroed, 512, 2);
rt_mallocx_nallocx!(rt_pow2_512bytes_2align_mallocx_nallocx, 512, 2);
rt_alloc_layout_checked!(rt_pow2_512bytes_2align_alloc_layout_checked, 512, 2);
rt_alloc_layout_unchecked!(rt_pow2_512bytes_2align_alloc_layout_unchecked, 512, 2);
rt_alloc_excess_unused!(rt_pow2_512bytes_2align_alloc_excess_unused, 512, 2);
rt_alloc_excess_used!(rt_pow2_512bytes_2align_alloc_excess_used, 512, 2);
rt_realloc_naive!(rt_pow2_512bytes_2align_realloc_naive, 512, 2);
rt_realloc!(rt_pow2_512bytes_2align_realloc, 512, 2);
rt_realloc_excess_unused!(rt_pow2_512bytes_2align_realloc_excess_unused, 512, 2);
rt_realloc_excess_used!(rt_pow2_512bytes_2align_realloc_excess_used, 512, 2);

rt_calloc!(rt_pow2_1024bytes_2align_calloc, 1024, 2);
rt_mallocx!(rt_pow2_1024bytes_2align_mallocx, 1024, 2);
rt_mallocx_zeroed!(rt_pow2_1024bytes_2align_mallocx_zeroed, 1024, 2);
rt_mallocx_nallocx!(rt_pow2_1024bytes_2align_mallocx_nallocx, 1024, 2);
rt_alloc_layout_checked!(rt_pow2_1024bytes_2align_alloc_layout_checked, 1024, 2);
rt_alloc_layout_unchecked!(rt_pow2_1024bytes_2align_alloc_layout_unchecked, 1024, 2);
rt_alloc_excess_unused!(rt_pow2_1024bytes_2align_alloc_excess_unused, 1024, 2);
rt_alloc_excess_used!(rt_pow2_1024bytes_2align_alloc_excess_used, 1024, 2);
rt_realloc_naive!(rt_pow2_1024bytes_2align_realloc_naive, 1024, 2);
rt_realloc!(rt_pow2_1024bytes_2align_realloc, 1024, 2);
rt_realloc_excess_unused!(rt_pow2_1024bytes_2align_realloc_excess_unused, 1024, 2);
rt_realloc_excess_used!(rt_pow2_1024bytes_2align_realloc_excess_used, 1024, 2);

rt_calloc!(rt_pow2_2048bytes_2align_calloc, 2048, 2);
rt_mallocx!(rt_pow2_2048bytes_2align_mallocx, 2048, 2);
rt_mallocx_zeroed!(rt_pow2_2048bytes_2align_mallocx_zeroed, 2048, 2);
rt_mallocx_nallocx!(rt_pow2_2048bytes_2align_mallocx_nallocx, 2048, 2);
rt_alloc_layout_checked!(rt_pow2_2048bytes_2align_alloc_layout_checked, 2048, 2);
rt_alloc_layout_unchecked!(rt_pow2_2048bytes_2align_alloc_layout_unchecked, 2048, 2);
rt_alloc_excess_unused!(rt_pow2_2048bytes_2align_alloc_excess_unused, 2048, 2);
rt_alloc_excess_used!(rt_pow2_2048bytes_2align_alloc_excess_used, 2048, 2);
rt_realloc_naive!(rt_pow2_2048bytes_2align_realloc_naive, 2048, 2);
rt_realloc!(rt_pow2_2048bytes_2align_realloc, 2048, 2);
rt_realloc_excess_unused!(rt_pow2_2048bytes_2align_realloc_excess_unused, 2048, 2);
rt_realloc_excess_used!(rt_pow2_2048bytes_2align_realloc_excess_used, 2048, 2);

rt_calloc!(rt_pow2_4096bytes_2align_calloc, 4096, 2);
rt_mallocx!(rt_pow2_4096bytes_2align_mallocx, 4096, 2);
rt_mallocx_zeroed!(rt_pow2_4096bytes_2align_mallocx_zeroed, 4096, 2);
rt_mallocx_nallocx!(rt_pow2_4096bytes_2align_mallocx_nallocx, 4096, 2);
rt_alloc_layout_checked!(rt_pow2_4096bytes_2align_alloc_layout_checked, 4096, 2);
rt_alloc_layout_unchecked!(rt_pow2_4096bytes_2align_alloc_layout_unchecked, 4096, 2);
rt_alloc_excess_unused!(rt_pow2_4096bytes_2align_alloc_excess_unused, 4096, 2);
rt_alloc_excess_used!(rt_pow2_4096bytes_2align_alloc_excess_used, 4096, 2);
rt_realloc_naive!(rt_pow2_4096bytes_2align_realloc_naive, 4096, 2);
rt_realloc!(rt_pow2_4096bytes_2align_realloc, 4096, 2);
rt_realloc_excess_unused!(rt_pow2_4096bytes_2align_realloc_excess_unused, 4096, 2);
rt_realloc_excess_used!(rt_pow2_4096bytes_2align_realloc_excess_used, 4096, 2);

rt_calloc!(rt_pow2_8192bytes_2align_calloc, 8192, 2);
rt_mallocx!(rt_pow2_8192bytes_2align_mallocx, 8192, 2);
rt_mallocx_zeroed!(rt_pow2_8192bytes_2align_mallocx_zeroed, 8192, 2);
rt_mallocx_nallocx!(rt_pow2_8192bytes_2align_mallocx_nallocx, 8192, 2);
rt_alloc_layout_checked!(rt_pow2_8192bytes_2align_alloc_layout_checked, 8192, 2);
rt_alloc_layout_unchecked!(rt_pow2_8192bytes_2align_alloc_layout_unchecked, 8192, 2);
rt_alloc_excess_unused!(rt_pow2_8192bytes_2align_alloc_excess_unused, 8192, 2);
rt_alloc_excess_used!(rt_pow2_8192bytes_2align_alloc_excess_used, 8192, 2);
rt_realloc_naive!(rt_pow2_8192bytes_2align_realloc_naive, 8192, 2);
rt_realloc!(rt_pow2_8192bytes_2align_realloc, 8192, 2);
rt_realloc_excess_unused!(rt_pow2_8192bytes_2align_realloc_excess_unused, 8192, 2);
rt_realloc_excess_used!(rt_pow2_8192bytes_2align_realloc_excess_used, 8192, 2);

rt_calloc!(rt_pow2_16384bytes_2align_calloc, 16384, 2);
rt_mallocx!(rt_pow2_16384bytes_2align_mallocx, 16384, 2);
rt_mallocx_zeroed!(rt_pow2_16384bytes_2align_mallocx_zeroed, 16384, 2);
rt_mallocx_nallocx!(rt_pow2_16384bytes_2align_mallocx_nallocx, 16384, 2);
rt_alloc_layout_checked!(rt_pow2_16384bytes_2align_alloc_layout_checked, 16384, 2);
rt_alloc_layout_unchecked!(rt_pow2_16384bytes_2align_alloc_layout_unchecked, 16384, 2);
rt_alloc_excess_unused!(rt_pow2_16384bytes_2align_alloc_excess_unused, 16384, 2);
rt_alloc_excess_used!(rt_pow2_16384bytes_2align_alloc_excess_used, 16384, 2);
rt_realloc_naive!(rt_pow2_16384bytes_2align_realloc_naive, 16384, 2);
rt_realloc!(rt_pow2_16384bytes_2align_realloc, 16384, 2);
rt_realloc_excess_unused!(rt_pow2_16384bytes_2align_realloc_excess_unused, 16384, 2);
rt_realloc_excess_used!(rt_pow2_16384bytes_2align_realloc_excess_used, 16384, 2);

rt_calloc!(rt_pow2_32768bytes_2align_calloc, 32768, 2);
rt_mallocx!(rt_pow2_32768bytes_2align_mallocx, 32768, 2);
rt_mallocx_zeroed!(rt_pow2_32768bytes_2align_mallocx_zeroed, 32768, 2);
rt_mallocx_nallocx!(rt_pow2_32768bytes_2align_mallocx_nallocx, 32768, 2);
rt_alloc_layout_checked!(rt_pow2_32768bytes_2align_alloc_layout_checked, 32768, 2);
rt_alloc_layout_unchecked!(rt_pow2_32768bytes_2align_alloc_layout_unchecked, 32768, 2);
rt_alloc_excess_unused!(rt_pow2_32768bytes_2align_alloc_excess_unused, 32768, 2);
rt_alloc_excess_used!(rt_pow2_32768bytes_2align_alloc_excess_used, 32768, 2);
rt_realloc_naive!(rt_pow2_32768bytes_2align_realloc_naive, 32768, 2);
rt_realloc!(rt_pow2_32768bytes_2align_realloc, 32768, 2);
rt_realloc_excess_unused!(rt_pow2_32768bytes_2align_realloc_excess_unused, 32768, 2);
rt_realloc_excess_used!(rt_pow2_32768bytes_2align_realloc_excess_used, 32768, 2);

rt_calloc!(rt_pow2_65536bytes_2align_calloc, 65536, 2);
rt_mallocx!(rt_pow2_65536bytes_2align_mallocx, 65536, 2);
rt_mallocx_zeroed!(rt_pow2_65536bytes_2align_mallocx_zeroed, 65536, 2);
rt_mallocx_nallocx!(rt_pow2_65536bytes_2align_mallocx_nallocx, 65536, 2);
rt_alloc_layout_checked!(rt_pow2_65536bytes_2align_alloc_layout_checked, 65536, 2);
rt_alloc_layout_unchecked!(rt_pow2_65536bytes_2align_alloc_layout_unchecked, 65536, 2);
rt_alloc_excess_unused!(rt_pow2_65536bytes_2align_alloc_excess_unused, 65536, 2);
rt_alloc_excess_used!(rt_pow2_65536bytes_2align_alloc_excess_used, 65536, 2);
rt_realloc_naive!(rt_pow2_65536bytes_2align_realloc_naive, 65536, 2);
rt_realloc!(rt_pow2_65536bytes_2align_realloc, 65536, 2);
rt_realloc_excess_unused!(rt_pow2_65536bytes_2align_realloc_excess_unused, 65536, 2);
rt_realloc_excess_used!(rt_pow2_65536bytes_2align_realloc_excess_used, 65536, 2);

rt_calloc!(rt_pow2_131072bytes_2align_calloc, 131072, 2);
rt_mallocx!(rt_pow2_131072bytes_2align_mallocx, 131072, 2);
rt_mallocx_zeroed!(rt_pow2_131072bytes_2align_mallocx_zeroed, 131072, 2);
rt_mallocx_nallocx!(rt_pow2_131072bytes_2align_mallocx_nallocx, 131072, 2);
rt_alloc_layout_checked!(rt_pow2_131072bytes_2align_alloc_layout_checked, 131072, 2);
rt_alloc_layout_unchecked!(rt_pow2_131072bytes_2align_alloc_layout_unchecked, 131072, 2);
rt_alloc_excess_unused!(rt_pow2_131072bytes_2align_alloc_excess_unused, 131072, 2);
rt_alloc_excess_used!(rt_pow2_131072bytes_2align_alloc_excess_used, 131072, 2);
rt_realloc_naive!(rt_pow2_131072bytes_2align_realloc_naive, 131072, 2);
rt_realloc!(rt_pow2_131072bytes_2align_realloc, 131072, 2);
rt_realloc_excess_unused!(rt_pow2_131072bytes_2align_realloc_excess_unused, 131072, 2);
rt_realloc_excess_used!(rt_pow2_131072bytes_2align_realloc_excess_used, 131072, 2);

rt_calloc!(rt_pow2_4194304bytes_2align_calloc, 4194304, 2);
rt_mallocx!(rt_pow2_4194304bytes_2align_mallocx, 4194304, 2);
rt_mallocx_zeroed!(rt_pow2_4194304bytes_2align_mallocx_zeroed, 4194304, 2);
rt_mallocx_nallocx!(rt_pow2_4194304bytes_2align_mallocx_nallocx, 4194304, 2);
rt_alloc_layout_checked!(rt_pow2_4194304bytes_2align_alloc_layout_checked, 4194304, 2);
rt_alloc_layout_unchecked!(
    rt_pow2_4194304bytes_2align_alloc_layout_unchecked,
    4194304,
    2
);
rt_alloc_excess_unused!(rt_pow2_4194304bytes_2align_alloc_excess_unused, 4194304, 2);
rt_alloc_excess_used!(rt_pow2_4194304bytes_2align_alloc_excess_used, 4194304, 2);
rt_realloc_naive!(rt_pow2_4194304bytes_2align_realloc_naive, 4194304, 2);
rt_realloc!(rt_pow2_4194304bytes_2align_realloc, 4194304, 2);
rt_realloc_excess_unused!(
    rt_pow2_4194304bytes_2align_realloc_excess_unused,
    4194304,
    2
);
rt_realloc_excess_used!(rt_pow2_4194304bytes_2align_realloc_excess_used, 4194304, 2);

// Even
rt_calloc!(rt_even_10bytes_2align_calloc, 10, 2);
rt_mallocx!(rt_even_10bytes_2align_mallocx, 10, 2);
rt_mallocx_zeroed!(rt_even_10bytes_2align_mallocx_zeroed, 10, 2);
rt_mallocx_nallocx!(rt_even_10bytes_2align_mallocx_nallocx, 10, 2);
rt_alloc_layout_checked!(rt_even_10bytes_2align_alloc_layout_checked, 10, 2);
rt_alloc_layout_unchecked!(rt_even_10bytes_2align_alloc_layout_unchecked, 10, 2);
rt_alloc_excess_unused!(rt_even_10bytes_2align_alloc_excess_unused, 10, 2);
rt_alloc_excess_used!(rt_even_10bytes_2align_alloc_excess_used, 10, 2);
rt_realloc_naive!(rt_even_10bytes_2align_realloc_naive, 10, 2);
rt_realloc!(rt_even_10bytes_2align_realloc, 10, 2);
rt_realloc_excess_unused!(rt_even_10bytes_2align_realloc_excess_unused, 10, 2);
rt_realloc_excess_used!(rt_even_10bytes_2align_realloc_excess_used, 10, 2);

rt_calloc!(rt_even_100bytes_2align_calloc, 100, 2);
rt_mallocx!(rt_even_100bytes_2align_mallocx, 100, 2);
rt_mallocx_zeroed!(rt_even_100bytes_2align_mallocx_zeroed, 100, 2);
rt_mallocx_nallocx!(rt_even_100bytes_2align_mallocx_nallocx, 100, 2);
rt_alloc_layout_checked!(rt_even_100bytes_2align_alloc_layout_checked, 100, 2);
rt_alloc_layout_unchecked!(rt_even_100bytes_2align_alloc_layout_unchecked, 100, 2);
rt_alloc_excess_unused!(rt_even_100bytes_2align_alloc_excess_unused, 100, 2);
rt_alloc_excess_used!(rt_even_100bytes_2align_alloc_excess_used, 100, 2);
rt_realloc_naive!(rt_even_100bytes_2align_realloc_naive, 100, 2);
rt_realloc!(rt_even_100bytes_2align_realloc, 100, 2);
rt_realloc_excess_unused!(rt_even_100bytes_2align_realloc_excess_unused, 100, 2);
rt_realloc_excess_used!(rt_even_100bytes_2align_realloc_excess_used, 100, 2);

rt_calloc!(rt_even_1000bytes_2align_calloc, 1000, 2);
rt_mallocx!(rt_even_1000bytes_2align_mallocx, 1000, 2);
rt_mallocx_zeroed!(rt_even_1000bytes_2align_mallocx_zeroed, 1000, 2);
rt_mallocx_nallocx!(rt_even_1000bytes_2align_mallocx_nallocx, 1000, 2);
rt_alloc_layout_checked!(rt_even_1000bytes_2align_alloc_layout_checked, 1000, 2);
rt_alloc_layout_unchecked!(rt_even_1000bytes_2align_alloc_layout_unchecked, 1000, 2);
rt_alloc_excess_unused!(rt_even_1000bytes_2align_alloc_excess_unused, 1000, 2);
rt_alloc_excess_used!(rt_even_1000bytes_2align_alloc_excess_used, 1000, 2);
rt_realloc_naive!(rt_even_1000bytes_2align_realloc_naive, 1000, 2);
rt_realloc!(rt_even_1000bytes_2align_realloc, 1000, 2);
rt_realloc_excess_unused!(rt_even_1000bytes_2align_realloc_excess_unused, 1000, 2);
rt_realloc_excess_used!(rt_even_1000bytes_2align_realloc_excess_used, 1000, 2);

rt_calloc!(rt_even_10000bytes_2align_calloc, 10000, 2);
rt_mallocx!(rt_even_10000bytes_2align_mallocx, 10000, 2);
rt_mallocx_zeroed!(rt_even_10000bytes_2align_mallocx_zeroed, 10000, 2);
rt_mallocx_nallocx!(rt_even_10000bytes_2align_mallocx_nallocx, 10000, 2);
rt_alloc_layout_checked!(rt_even_10000bytes_2align_alloc_layout_checked, 10000, 2);
rt_alloc_layout_unchecked!(rt_even_10000bytes_2align_alloc_layout_unchecked, 10000, 2);
rt_alloc_excess_unused!(rt_even_10000bytes_2align_alloc_excess_unused, 10000, 2);
rt_alloc_excess_used!(rt_even_10000bytes_2align_alloc_excess_used, 10000, 2);
rt_realloc_naive!(rt_even_10000bytes_2align_realloc_naive, 10000, 2);
rt_realloc!(rt_even_10000bytes_2align_realloc, 10000, 2);
rt_realloc_excess_unused!(rt_even_10000bytes_2align_realloc_excess_unused, 10000, 2);
rt_realloc_excess_used!(rt_even_10000bytes_2align_realloc_excess_used, 10000, 2);

rt_calloc!(rt_even_100000bytes_2align_calloc, 100000, 2);
rt_mallocx!(rt_even_100000bytes_2align_mallocx, 100000, 2);
rt_mallocx_zeroed!(rt_even_100000bytes_2align_mallocx_zeroed, 100000, 2);
rt_mallocx_nallocx!(rt_even_100000bytes_2align_mallocx_nallocx, 100000, 2);
rt_alloc_layout_checked!(rt_even_100000bytes_2align_alloc_layout_checked, 100000, 2);
rt_alloc_layout_unchecked!(rt_even_100000bytes_2align_alloc_layout_unchecked, 100000, 2);
rt_alloc_excess_unused!(rt_even_100000bytes_2align_alloc_excess_unused, 100000, 2);
rt_alloc_excess_used!(rt_even_100000bytes_2align_alloc_excess_used, 100000, 2);
rt_realloc_naive!(rt_even_100000bytes_2align_realloc_naive, 100000, 2);
rt_realloc!(rt_even_100000bytes_2align_realloc, 100000, 2);
rt_realloc_excess_unused!(rt_even_100000bytes_2align_realloc_excess_unused, 100000, 2);
rt_realloc_excess_used!(rt_even_100000bytes_2align_realloc_excess_used, 100000, 2);

rt_calloc!(rt_even_1000000bytes_2align_calloc, 1000000, 2);
rt_mallocx!(rt_even_1000000bytes_2align_mallocx, 1000000, 2);
rt_mallocx_zeroed!(rt_even_1000000bytes_2align_mallocx_zeroed, 1000000, 2);
rt_mallocx_nallocx!(rt_even_1000000bytes_2align_mallocx_nallocx, 1000000, 2);
rt_alloc_layout_checked!(rt_even_1000000bytes_2align_alloc_layout_checked, 1000000, 2);
rt_alloc_layout_unchecked!(
    rt_even_1000000bytes_2align_alloc_layout_unchecked,
    1000000,
    2
);
rt_alloc_excess_unused!(rt_even_1000000bytes_2align_alloc_excess_unused, 1000000, 2);
rt_alloc_excess_used!(rt_even_1000000bytes_2align_alloc_excess_used, 1000000, 2);
rt_realloc_naive!(rt_even_1000000bytes_2align_realloc_naive, 1000000, 2);
rt_realloc!(rt_even_1000000bytes_2align_realloc, 1000000, 2);
rt_realloc_excess_unused!(
    rt_even_1000000bytes_2align_realloc_excess_unused,
    1000000,
    2
);
rt_realloc_excess_used!(rt_even_1000000bytes_2align_realloc_excess_used, 1000000, 2);

// Odd:
rt_calloc!(rt_odd_10bytes_2align_calloc, 10 - 1, 2);
rt_mallocx!(rt_odd_10bytes_2align_mallocx, 10 - 1, 2);
rt_mallocx_zeroed!(rt_odd_10bytes_2align_mallocx_zeroed, 10 - 1, 2);
rt_mallocx_nallocx!(rt_odd_10bytes_2align_mallocx_nallocx, 10 - 1, 2);
rt_alloc_layout_checked!(rt_odd_10bytes_2align_alloc_layout_checked, 10 - 1, 2);
rt_alloc_layout_unchecked!(rt_odd_10bytes_2align_alloc_layout_unchecked, 10 - 1, 2);
rt_alloc_excess_unused!(rt_odd_10bytes_2align_alloc_excess_unused, 10 - 1, 2);
rt_alloc_excess_used!(rt_odd_10bytes_2align_alloc_excess_used, 10 - 1, 2);
rt_realloc_naive!(rt_odd_10bytes_2align_realloc_naive, 10 - 1, 2);
rt_realloc!(rt_odd_10bytes_2align_realloc, 10 - 1, 2);
rt_realloc_excess_unused!(rt_odd_10bytes_2align_realloc_excess_unused, 10 - 1, 2);
rt_realloc_excess_used!(rt_odd_10bytes_2align_realloc_excess_used, 10 - 1, 2);

rt_calloc!(rt_odd_100bytes_2align_calloc, 100 - 1, 2);
rt_mallocx!(rt_odd_100bytes_2align_mallocx, 100 - 1, 2);
rt_mallocx_zeroed!(rt_odd_100bytes_2align_mallocx_zeroed, 100 - 1, 2);
rt_mallocx_nallocx!(rt_odd_100bytes_2align_mallocx_nallocx, 100 - 1, 2);
rt_alloc_layout_checked!(rt_odd_100bytes_2align_alloc_layout_checked, 100 - 1, 2);
rt_alloc_layout_unchecked!(rt_odd_100bytes_2align_alloc_layout_unchecked, 100 - 1, 2);
rt_alloc_excess_unused!(rt_odd_100bytes_2align_alloc_excess_unused, 100 - 1, 2);
rt_alloc_excess_used!(rt_odd_100bytes_2align_alloc_excess_used, 100 - 1, 2);
rt_realloc_naive!(rt_odd_100bytes_2align_realloc_naive, 100 - 1, 2);
rt_realloc!(rt_odd_100bytes_2align_realloc, 100 - 1, 2);
rt_realloc_excess_unused!(rt_odd_100bytes_2align_realloc_excess_unused, 100 - 1, 2);
rt_realloc_excess_used!(rt_odd_100bytes_2align_realloc_excess_used, 100 - 1, 2);

rt_calloc!(rt_odd_1000bytes_2align_calloc, 1000 - 1, 2);
rt_mallocx!(rt_odd_1000bytes_2align_mallocx, 1000 - 1, 2);
rt_mallocx_zeroed!(rt_odd_1000bytes_2align_mallocx_zeroed, 1000 - 1, 2);
rt_mallocx_nallocx!(rt_odd_1000bytes_2align_mallocx_nallocx, 1000 - 1, 2);
rt_alloc_layout_checked!(rt_odd_1000bytes_2align_alloc_layout_checked, 1000 - 1, 2);
rt_alloc_layout_unchecked!(rt_odd_1000bytes_2align_alloc_layout_unchecked, 1000 - 1, 2);
rt_alloc_excess_unused!(rt_odd_1000bytes_2align_alloc_excess_unused, 1000 - 1, 2);
rt_alloc_excess_used!(rt_odd_1000bytes_2align_alloc_excess_used, 1000 - 1, 2);
rt_realloc_naive!(rt_odd_1000bytes_2align_realloc_naive, 1000 - 1, 2);
rt_realloc!(rt_odd_1000bytes_2align_realloc, 1000 - 1, 2);
rt_realloc_excess_unused!(rt_odd_1000bytes_2align_realloc_excess_unused, 1000 - 1, 2);
rt_realloc_excess_used!(rt_odd_1000bytes_2align_realloc_excess_used, 1000 - 1, 2);

rt_calloc!(rt_odd_10000bytes_2align_calloc, 10000 - 1, 2);
rt_mallocx!(rt_odd_10000bytes_2align_mallocx, 10000 - 1, 2);
rt_mallocx_zeroed!(rt_odd_10000bytes_2align_mallocx_zeroed, 10000 - 1, 2);
rt_mallocx_nallocx!(rt_odd_10000bytes_2align_mallocx_nallocx, 10000 - 1, 2);
rt_alloc_layout_checked!(rt_odd_10000bytes_2align_alloc_layout_checked, 10000 - 1, 2);
rt_alloc_layout_unchecked!(
    rt_odd_10000bytes_2align_alloc_layout_unchecked,
    10000 - 1,
    2
);
rt_alloc_excess_unused!(rt_odd_10000bytes_2align_alloc_excess_unused, 10000 - 1, 2);
rt_alloc_excess_used!(rt_odd_10000bytes_2align_alloc_excess_used, 10000 - 1, 2);
rt_realloc_naive!(rt_odd_10000bytes_2align_realloc_naive, 10000 - 1, 2);
rt_realloc!(rt_odd_10000bytes_2align_realloc, 10000 - 1, 2);
rt_realloc_excess_unused!(rt_odd_10000bytes_2align_realloc_excess_unused, 10000 - 1, 2);
rt_realloc_excess_used!(rt_odd_10000bytes_2align_realloc_excess_used, 10000 - 1, 2);

rt_calloc!(rt_odd_100000bytes_2align_calloc, 100000 - 1, 2);
rt_mallocx!(rt_odd_100000bytes_2align_mallocx, 100000 - 1, 2);
rt_mallocx_zeroed!(rt_odd_100000bytes_2align_mallocx_zeroed, 100000 - 1, 2);
rt_mallocx_nallocx!(rt_odd_100000bytes_2align_mallocx_nallocx, 100000 - 1, 2);
rt_alloc_layout_checked!(
    rt_odd_100000bytes_2align_alloc_layout_checked,
    100000 - 1,
    2
);
rt_alloc_layout_unchecked!(
    rt_odd_100000bytes_2align_alloc_layout_unchecked,
    100000 - 1,
    2
);
rt_alloc_excess_unused!(rt_odd_100000bytes_2align_alloc_excess_unused, 100000 - 1, 2);
rt_alloc_excess_used!(rt_odd_100000bytes_2align_alloc_excess_used, 100000 - 1, 2);
rt_realloc_naive!(rt_odd_100000bytes_2align_realloc_naive, 100000 - 1, 2);
rt_realloc!(rt_odd_100000bytes_2align_realloc, 100000 - 1, 2);
rt_realloc_excess_unused!(
    rt_odd_100000bytes_2align_realloc_excess_unused,
    100000 - 1,
    2
);
rt_realloc_excess_used!(rt_odd_100000bytes_2align_realloc_excess_used, 100000 - 1, 2);

rt_calloc!(rt_odd_1000000bytes_2align_calloc, 1000000 - 1, 2);
rt_mallocx!(rt_odd_1000000bytes_2align_mallocx, 1000000 - 1, 2);
rt_mallocx_zeroed!(rt_odd_1000000bytes_2align_mallocx_zeroed, 1000000 - 1, 2);
rt_mallocx_nallocx!(rt_odd_1000000bytes_2align_mallocx_nallocx, 1000000 - 1, 2);
rt_alloc_layout_checked!(
    rt_odd_1000000bytes_2align_alloc_layout_checked,
    1000000 - 1,
    2
);
rt_alloc_layout_unchecked!(
    rt_odd_1000000bytes_2align_alloc_layout_unchecked,
    1000000 - 1,
    2
);
rt_alloc_excess_unused!(
    rt_odd_1000000bytes_2align_alloc_excess_unused,
    1000000 - 1,
    2
);
rt_alloc_excess_used!(rt_odd_1000000bytes_2align_alloc_excess_used, 1000000 - 1, 2);
rt_realloc_naive!(rt_odd_1000000bytes_2align_realloc_naive, 1000000 - 1, 2);
rt_realloc!(rt_odd_1000000bytes_2align_realloc, 1000000 - 1, 2);
rt_realloc_excess_unused!(
    rt_odd_1000000bytes_2align_realloc_excess_unused,
    1000000 - 1,
    2
);
rt_realloc_excess_used!(
    rt_odd_1000000bytes_2align_realloc_excess_used,
    1000000 - 1,
    2
);

// primes
rt_calloc!(rt_primes_3bytes_2align_calloc, 3, 2);
rt_mallocx!(rt_primes_3bytes_2align_mallocx, 3, 2);
rt_mallocx_zeroed!(rt_primes_3bytes_2align_mallocx_zeroed, 3, 2);
rt_mallocx_nallocx!(rt_primes_3bytes_2align_mallocx_nallocx, 3, 2);
rt_alloc_layout_checked!(rt_primes_3bytes_2align_alloc_layout_checked, 3, 2);
rt_alloc_layout_unchecked!(rt_primes_3bytes_2align_alloc_layout_unchecked, 3, 2);
rt_alloc_excess_unused!(rt_primes_3bytes_2align_alloc_excess_unused, 3, 2);
rt_alloc_excess_used!(rt_primes_3bytes_2align_alloc_excess_used, 3, 2);
rt_realloc_naive!(rt_primes_3bytes_2align_realloc_naive, 3, 2);
rt_realloc!(rt_primes_3bytes_2align_realloc, 3, 2);
rt_realloc_excess_unused!(rt_primes_3bytes_2align_realloc_excess_unused, 3, 2);
rt_realloc_excess_used!(rt_primes_3bytes_2align_realloc_excess_used, 3, 2);

rt_calloc!(rt_primes_7bytes_2align_calloc, 7, 2);
rt_mallocx!(rt_primes_7bytes_2align_mallocx, 7, 2);
rt_mallocx_zeroed!(rt_primes_7bytes_2align_mallocx_zeroed, 7, 2);
rt_mallocx_nallocx!(rt_primes_7bytes_2align_mallocx_nallocx, 7, 2);
rt_alloc_layout_checked!(rt_primes_7bytes_2align_alloc_layout_checked, 7, 2);
rt_alloc_layout_unchecked!(rt_primes_7bytes_2align_alloc_layout_unchecked, 7, 2);
rt_alloc_excess_unused!(rt_primes_7bytes_2align_alloc_excess_unused, 7, 2);
rt_alloc_excess_used!(rt_primes_7bytes_2align_alloc_excess_used, 7, 2);
rt_realloc_naive!(rt_primes_7bytes_2align_realloc_naive, 7, 2);
rt_realloc!(rt_primes_7bytes_2align_realloc, 7, 2);
rt_realloc_excess_unused!(rt_primes_7bytes_2align_realloc_excess_unused, 7, 2);
rt_realloc_excess_used!(rt_primes_7bytes_2align_realloc_excess_used, 7, 2);

rt_calloc!(rt_primes_13bytes_2align_calloc, 13, 2);
rt_mallocx!(rt_primes_13bytes_2align_mallocx, 13, 2);
rt_mallocx_zeroed!(rt_primes_13bytes_2align_mallocx_zeroed, 13, 2);
rt_mallocx_nallocx!(rt_primes_13bytes_2align_mallocx_nallocx, 13, 2);
rt_alloc_layout_checked!(rt_primes_13bytes_2align_alloc_layout_checked, 13, 2);
rt_alloc_layout_unchecked!(rt_primes_13bytes_2align_alloc_layout_unchecked, 13, 2);
rt_alloc_excess_unused!(rt_primes_13bytes_2align_alloc_excess_unused, 13, 2);
rt_alloc_excess_used!(rt_primes_13bytes_2align_alloc_excess_used, 13, 2);
rt_realloc_naive!(rt_primes_13bytes_2align_realloc_naive, 13, 2);
rt_realloc!(rt_primes_13bytes_2align_realloc, 13, 2);
rt_realloc_excess_unused!(rt_primes_13bytes_2align_realloc_excess_unused, 13, 2);
rt_realloc_excess_used!(rt_primes_13bytes_2align_realloc_excess_used, 13, 2);

rt_calloc!(rt_primes_17bytes_2align_calloc, 17, 2);
rt_mallocx!(rt_primes_17bytes_2align_mallocx, 17, 2);
rt_mallocx_zeroed!(rt_primes_17bytes_2align_mallocx_zeroed, 17, 2);
rt_mallocx_nallocx!(rt_primes_17bytes_2align_mallocx_nallocx, 17, 2);
rt_alloc_layout_checked!(rt_primes_17bytes_2align_alloc_layout_checked, 17, 2);
rt_alloc_layout_unchecked!(rt_primes_17bytes_2align_alloc_layout_unchecked, 17, 2);
rt_alloc_excess_unused!(rt_primes_17bytes_2align_alloc_excess_unused, 17, 2);
rt_alloc_excess_used!(rt_primes_17bytes_2align_alloc_excess_used, 17, 2);
rt_realloc_naive!(rt_primes_17bytes_2align_realloc_naive, 17, 2);
rt_realloc!(rt_primes_17bytes_2align_realloc, 17, 2);
rt_realloc_excess_unused!(rt_primes_17bytes_2align_realloc_excess_unused, 17, 2);
rt_realloc_excess_used!(rt_primes_17bytes_2align_realloc_excess_used, 17, 2);

rt_calloc!(rt_primes_31bytes_2align_calloc, 31, 2);
rt_mallocx!(rt_primes_31bytes_2align_mallocx, 31, 2);
rt_mallocx_zeroed!(rt_primes_31bytes_2align_mallocx_zeroed, 31, 2);
rt_mallocx_nallocx!(rt_primes_31bytes_2align_mallocx_nallocx, 31, 2);
rt_alloc_layout_checked!(rt_primes_31bytes_2align_alloc_layout_checked, 31, 2);
rt_alloc_layout_unchecked!(rt_primes_31bytes_2align_alloc_layout_unchecked, 31, 2);
rt_alloc_excess_unused!(rt_primes_31bytes_2align_alloc_excess_unused, 31, 2);
rt_alloc_excess_used!(rt_primes_31bytes_2align_alloc_excess_used, 31, 2);
rt_realloc_naive!(rt_primes_31bytes_2align_realloc_naive, 31, 2);
rt_realloc!(rt_primes_31bytes_2align_realloc, 31, 2);
rt_realloc_excess_unused!(rt_primes_31bytes_2align_realloc_excess_unused, 31, 2);
rt_realloc_excess_used!(rt_primes_31bytes_2align_realloc_excess_used, 31, 2);

rt_calloc!(rt_primes_61bytes_2align_calloc, 61, 2);
rt_mallocx!(rt_primes_61bytes_2align_mallocx, 61, 2);
rt_mallocx_zeroed!(rt_primes_61bytes_2align_mallocx_zeroed, 61, 2);
rt_mallocx_nallocx!(rt_primes_61bytes_2align_mallocx_nallocx, 61, 2);
rt_alloc_layout_checked!(rt_primes_61bytes_2align_alloc_layout_checked, 61, 2);
rt_alloc_layout_unchecked!(rt_primes_61bytes_2align_alloc_layout_unchecked, 61, 2);
rt_alloc_excess_unused!(rt_primes_61bytes_2align_alloc_excess_unused, 61, 2);
rt_alloc_excess_used!(rt_primes_61bytes_2align_alloc_excess_used, 61, 2);
rt_realloc_naive!(rt_primes_61bytes_2align_realloc_naive, 61, 2);
rt_realloc!(rt_primes_61bytes_2align_realloc, 61, 2);
rt_realloc_excess_unused!(rt_primes_61bytes_2align_realloc_excess_unused, 61, 2);
rt_realloc_excess_used!(rt_primes_61bytes_2align_realloc_excess_used, 61, 2);

rt_calloc!(rt_primes_96bytes_2align_calloc, 96, 2);
rt_mallocx!(rt_primes_96bytes_2align_mallocx, 96, 2);
rt_mallocx_zeroed!(rt_primes_96bytes_2align_mallocx_zeroed, 96, 2);
rt_mallocx_nallocx!(rt_primes_96bytes_2align_mallocx_nallocx, 96, 2);
rt_alloc_layout_checked!(rt_primes_96bytes_2align_alloc_layout_checked, 96, 2);
rt_alloc_layout_unchecked!(rt_primes_96bytes_2align_alloc_layout_unchecked, 96, 2);
rt_alloc_excess_unused!(rt_primes_96bytes_2align_alloc_excess_unused, 96, 2);
rt_alloc_excess_used!(rt_primes_96bytes_2align_alloc_excess_used, 96, 2);
rt_realloc_naive!(rt_primes_96bytes_2align_realloc_naive, 96, 2);
rt_realloc!(rt_primes_96bytes_2align_realloc, 96, 2);
rt_realloc_excess_unused!(rt_primes_96bytes_2align_realloc_excess_unused, 96, 2);
rt_realloc_excess_used!(rt_primes_96bytes_2align_realloc_excess_used, 96, 2);

rt_calloc!(rt_primes_127bytes_2align_calloc, 127, 2);
rt_mallocx!(rt_primes_127bytes_2align_mallocx, 127, 2);
rt_mallocx_zeroed!(rt_primes_127bytes_2align_mallocx_zeroed, 127, 2);
rt_mallocx_nallocx!(rt_primes_127bytes_2align_mallocx_nallocx, 127, 2);
rt_alloc_layout_checked!(rt_primes_127bytes_2align_alloc_layout_checked, 127, 2);
rt_alloc_layout_unchecked!(rt_primes_127bytes_2align_alloc_layout_unchecked, 127, 2);
rt_alloc_excess_unused!(rt_primes_127bytes_2align_alloc_excess_unused, 127, 2);
rt_alloc_excess_used!(rt_primes_127bytes_2align_alloc_excess_used, 127, 2);
rt_realloc_naive!(rt_primes_127bytes_2align_realloc_naive, 127, 2);
rt_realloc!(rt_primes_127bytes_2align_realloc, 127, 2);
rt_realloc_excess_unused!(rt_primes_127bytes_2align_realloc_excess_unused, 127, 2);
rt_realloc_excess_used!(rt_primes_127bytes_2align_realloc_excess_used, 127, 2);

rt_calloc!(rt_primes_257bytes_2align_calloc, 257, 2);
rt_mallocx!(rt_primes_257bytes_2align_mallocx, 257, 2);
rt_mallocx_zeroed!(rt_primes_257bytes_2align_mallocx_zeroed, 257, 2);
rt_mallocx_nallocx!(rt_primes_257bytes_2align_mallocx_nallocx, 257, 2);
rt_alloc_layout_checked!(rt_primes_257bytes_2align_alloc_layout_checked, 257, 2);
rt_alloc_layout_unchecked!(rt_primes_257bytes_2align_alloc_layout_unchecked, 257, 2);
rt_alloc_excess_unused!(rt_primes_257bytes_2align_alloc_excess_unused, 257, 2);
rt_alloc_excess_used!(rt_primes_257bytes_2align_alloc_excess_used, 257, 2);
rt_realloc_naive!(rt_primes_257bytes_2align_realloc_naive, 257, 2);
rt_realloc!(rt_primes_257bytes_2align_realloc, 257, 2);
rt_realloc_excess_unused!(rt_primes_257bytes_2align_realloc_excess_unused, 257, 2);
rt_realloc_excess_used!(rt_primes_257bytes_2align_realloc_excess_used, 257, 2);

rt_calloc!(rt_primes_509bytes_2align_calloc, 509, 2);
rt_mallocx!(rt_primes_509bytes_2align_mallocx, 509, 2);
rt_mallocx_zeroed!(rt_primes_509bytes_2align_mallocx_zeroed, 509, 2);
rt_mallocx_nallocx!(rt_primes_509bytes_2align_mallocx_nallocx, 509, 2);
rt_alloc_layout_checked!(rt_primes_509bytes_2align_alloc_layout_checked, 509, 2);
rt_alloc_layout_unchecked!(rt_primes_509bytes_2align_alloc_layout_unchecked, 509, 2);
rt_alloc_excess_unused!(rt_primes_509bytes_2align_alloc_excess_unused, 509, 2);
rt_alloc_excess_used!(rt_primes_509bytes_2align_alloc_excess_used, 509, 2);
rt_realloc_naive!(rt_primes_509bytes_2align_realloc_naive, 509, 2);
rt_realloc!(rt_primes_509bytes_2align_realloc, 509, 2);
rt_realloc_excess_unused!(rt_primes_509bytes_2align_realloc_excess_unused, 509, 2);
rt_realloc_excess_used!(rt_primes_509bytes_2align_realloc_excess_used, 509, 2);

rt_calloc!(rt_primes_1021bytes_2align_calloc, 1021, 2);
rt_mallocx!(rt_primes_1021bytes_2align_mallocx, 1021, 2);
rt_mallocx_zeroed!(rt_primes_1021bytes_2align_mallocx_zeroed, 1021, 2);
rt_mallocx_nallocx!(rt_primes_1021bytes_2align_mallocx_nallocx, 1021, 2);
rt_alloc_layout_checked!(rt_primes_1021bytes_2align_alloc_layout_checked, 1021, 2);
rt_alloc_layout_unchecked!(rt_primes_1021bytes_2align_alloc_layout_unchecked, 1021, 2);
rt_alloc_excess_unused!(rt_primes_1021bytes_2align_alloc_excess_unused, 1021, 2);
rt_alloc_excess_used!(rt_primes_1021bytes_2align_alloc_excess_used, 1021, 2);
rt_realloc_naive!(rt_primes_1021bytes_2align_realloc_naive, 1021, 2);
rt_realloc!(rt_primes_1021bytes_2align_realloc, 1021, 2);
rt_realloc_excess_unused!(rt_primes_1021bytes_2align_realloc_excess_unused, 1021, 2);
rt_realloc_excess_used!(rt_primes_1021bytes_2align_realloc_excess_used, 1021, 2);

rt_calloc!(rt_primes_2039bytes_2align_calloc, 2039, 2);
rt_mallocx!(rt_primes_2039bytes_2align_mallocx, 2039, 2);
rt_mallocx_zeroed!(rt_primes_2039bytes_2align_mallocx_zeroed, 2039, 2);
rt_mallocx_nallocx!(rt_primes_2039bytes_2align_mallocx_nallocx, 2039, 2);
rt_alloc_layout_checked!(rt_primes_2039bytes_2align_alloc_layout_checked, 2039, 2);
rt_alloc_layout_unchecked!(rt_primes_2039bytes_2align_alloc_layout_unchecked, 2039, 2);
rt_alloc_excess_unused!(rt_primes_2039bytes_2align_alloc_excess_unused, 2039, 2);
rt_alloc_excess_used!(rt_primes_2039bytes_2align_alloc_excess_used, 2039, 2);
rt_realloc_naive!(rt_primes_2039bytes_2align_realloc_naive, 2039, 2);
rt_realloc!(rt_primes_2039bytes_2align_realloc, 2039, 2);
rt_realloc_excess_unused!(rt_primes_2039bytes_2align_realloc_excess_unused, 2039, 2);
rt_realloc_excess_used!(rt_primes_2039bytes_2align_realloc_excess_used, 2039, 2);

rt_calloc!(rt_primes_4093bytes_2align_calloc, 4093, 2);
rt_mallocx!(rt_primes_4093bytes_2align_mallocx, 4093, 2);
rt_mallocx_zeroed!(rt_primes_4093bytes_2align_mallocx_zeroed, 4093, 2);
rt_mallocx_nallocx!(rt_primes_4093bytes_2align_mallocx_nallocx, 4093, 2);
rt_alloc_layout_checked!(rt_primes_4093bytes_2align_alloc_layout_checked, 4093, 2);
rt_alloc_layout_unchecked!(rt_primes_4093bytes_2align_alloc_layout_unchecked, 4093, 2);
rt_alloc_excess_unused!(rt_primes_4093bytes_2align_alloc_excess_unused, 4093, 2);
rt_alloc_excess_used!(rt_primes_4093bytes_2align_alloc_excess_used, 4093, 2);
rt_realloc_naive!(rt_primes_4093bytes_2align_realloc_naive, 4093, 2);
rt_realloc!(rt_primes_4093bytes_2align_realloc, 4093, 2);
rt_realloc_excess_unused!(rt_primes_4093bytes_2align_realloc_excess_unused, 4093, 2);
rt_realloc_excess_used!(rt_primes_4093bytes_2align_realloc_excess_used, 4093, 2);

rt_calloc!(rt_primes_8191bytes_2align_calloc, 8191, 2);
rt_mallocx!(rt_primes_8191bytes_2align_mallocx, 8191, 2);
rt_mallocx_zeroed!(rt_primes_8191bytes_2align_mallocx_zeroed, 8191, 2);
rt_mallocx_nallocx!(rt_primes_8191bytes_2align_mallocx_nallocx, 8191, 2);
rt_alloc_layout_checked!(rt_primes_8191bytes_2align_alloc_layout_checked, 8191, 2);
rt_alloc_layout_unchecked!(rt_primes_8191bytes_2align_alloc_layout_unchecked, 8191, 2);
rt_alloc_excess_unused!(rt_primes_8191bytes_2align_alloc_excess_unused, 8191, 2);
rt_alloc_excess_used!(rt_primes_8191bytes_2align_alloc_excess_used, 8191, 2);
rt_realloc_naive!(rt_primes_8191bytes_2align_realloc_naive, 8191, 2);
rt_realloc!(rt_primes_8191bytes_2align_realloc, 8191, 2);
rt_realloc_excess_unused!(rt_primes_8191bytes_2align_realloc_excess_unused, 8191, 2);
rt_realloc_excess_used!(rt_primes_8191bytes_2align_realloc_excess_used, 8191, 2);

rt_calloc!(rt_primes_16381bytes_2align_calloc, 16381, 2);
rt_mallocx!(rt_primes_16381bytes_2align_mallocx, 16381, 2);
rt_mallocx_zeroed!(rt_primes_16381bytes_2align_mallocx_zeroed, 16381, 2);
rt_mallocx_nallocx!(rt_primes_16381bytes_2align_mallocx_nallocx, 16381, 2);
rt_alloc_layout_checked!(rt_primes_16381bytes_2align_alloc_layout_checked, 16381, 2);
rt_alloc_layout_unchecked!(rt_primes_16381bytes_2align_alloc_layout_unchecked, 16381, 2);
rt_alloc_excess_unused!(rt_primes_16381bytes_2align_alloc_excess_unused, 16381, 2);
rt_alloc_excess_used!(rt_primes_16381bytes_2align_alloc_excess_used, 16381, 2);
rt_realloc_naive!(rt_primes_16381bytes_2align_realloc_naive, 16381, 2);
rt_realloc!(rt_primes_16381bytes_2align_realloc, 16381, 2);
rt_realloc_excess_unused!(rt_primes_16381bytes_2align_realloc_excess_unused, 16381, 2);
rt_realloc_excess_used!(rt_primes_16381bytes_2align_realloc_excess_used, 16381, 2);

rt_calloc!(rt_primes_32749bytes_2align_calloc, 32749, 2);
rt_mallocx!(rt_primes_32749bytes_2align_mallocx, 32749, 2);
rt_mallocx_zeroed!(rt_primes_32749bytes_2align_mallocx_zeroed, 32749, 2);
rt_mallocx_nallocx!(rt_primes_32749bytes_2align_mallocx_nallocx, 32749, 2);
rt_alloc_layout_checked!(rt_primes_32749bytes_2align_alloc_layout_checked, 32749, 2);
rt_alloc_layout_unchecked!(rt_primes_32749bytes_2align_alloc_layout_unchecked, 32749, 2);
rt_alloc_excess_unused!(rt_primes_32749bytes_2align_alloc_excess_unused, 32749, 2);
rt_alloc_excess_used!(rt_primes_32749bytes_2align_alloc_excess_used, 32749, 2);
rt_realloc_naive!(rt_primes_32749bytes_2align_realloc_naive, 32749, 2);
rt_realloc!(rt_primes_32749bytes_2align_realloc, 32749, 2);
rt_realloc_excess_unused!(rt_primes_32749bytes_2align_realloc_excess_unused, 32749, 2);
rt_realloc_excess_used!(rt_primes_32749bytes_2align_realloc_excess_used, 32749, 2);

rt_calloc!(rt_primes_65537bytes_2align_calloc, 65537, 2);
rt_mallocx!(rt_primes_65537bytes_2align_mallocx, 65537, 2);
rt_mallocx_zeroed!(rt_primes_65537bytes_2align_mallocx_zeroed, 65537, 2);
rt_mallocx_nallocx!(rt_primes_65537bytes_2align_mallocx_nallocx, 65537, 2);
rt_alloc_layout_checked!(rt_primes_65537bytes_2align_alloc_layout_checked, 65537, 2);
rt_alloc_layout_unchecked!(rt_primes_65537bytes_2align_alloc_layout_unchecked, 65537, 2);
rt_alloc_excess_unused!(rt_primes_65537bytes_2align_alloc_excess_unused, 65537, 2);
rt_alloc_excess_used!(rt_primes_65537bytes_2align_alloc_excess_used, 65537, 2);
rt_realloc_naive!(rt_primes_65537bytes_2align_realloc_naive, 65537, 2);
rt_realloc!(rt_primes_65537bytes_2align_realloc, 65537, 2);
rt_realloc_excess_unused!(rt_primes_65537bytes_2align_realloc_excess_unused, 65537, 2);
rt_realloc_excess_used!(rt_primes_65537bytes_2align_realloc_excess_used, 65537, 2);

rt_calloc!(rt_primes_131071bytes_2align_calloc, 131071, 2);
rt_mallocx!(rt_primes_131071bytes_2align_mallocx, 131071, 2);
rt_mallocx_zeroed!(rt_primes_131071bytes_2align_mallocx_zeroed, 131071, 2);
rt_mallocx_nallocx!(rt_primes_131071bytes_2align_mallocx_nallocx, 131071, 2);
rt_alloc_layout_checked!(rt_primes_131071bytes_2align_alloc_layout_checked, 131071, 2);
rt_alloc_layout_unchecked!(
    rt_primes_131071bytes_2align_alloc_layout_unchecked,
    131071,
    2
);
rt_alloc_excess_unused!(rt_primes_131071bytes_2align_alloc_excess_unused, 131071, 2);
rt_alloc_excess_used!(rt_primes_131071bytes_2align_alloc_excess_used, 131071, 2);
rt_realloc_naive!(rt_primes_131071bytes_2align_realloc_naive, 131071, 2);
rt_realloc!(rt_primes_131071bytes_2align_realloc, 131071, 2);
rt_realloc_excess_unused!(
    rt_primes_131071bytes_2align_realloc_excess_unused,
    131071,
    2
);
rt_realloc_excess_used!(rt_primes_131071bytes_2align_realloc_excess_used, 131071, 2);

rt_calloc!(rt_primes_4194301bytes_2align_calloc, 4194301, 2);
rt_mallocx!(rt_primes_4194301bytes_2align_mallocx, 4194301, 2);
rt_mallocx_zeroed!(rt_primes_4194301bytes_2align_mallocx_zeroed, 4194301, 2);
rt_mallocx_nallocx!(rt_primes_4194301bytes_2align_mallocx_nallocx, 4194301, 2);
rt_alloc_layout_checked!(
    rt_primes_4194301bytes_2align_alloc_layout_checked,
    4194301,
    2
);
rt_alloc_layout_unchecked!(
    rt_primes_4194301bytes_2align_alloc_layout_unchecked,
    4194301,
    2
);
rt_alloc_excess_unused!(
    rt_primes_4194301bytes_2align_alloc_excess_unused,
    4194301,
    2
);
rt_alloc_excess_used!(rt_primes_4194301bytes_2align_alloc_excess_used, 4194301, 2);
rt_realloc_naive!(rt_primes_4194301bytes_2align_realloc_naive, 4194301, 2);
rt_realloc!(rt_primes_4194301bytes_2align_realloc, 4194301, 2);
rt_realloc_excess_unused!(
    rt_primes_4194301bytes_2align_realloc_excess_unused,
    4194301,
    2
);
rt_realloc_excess_used!(
    rt_primes_4194301bytes_2align_realloc_excess_used,
    4194301,
    2
);

// 4 bytes alignment

// Powers of two:
rt_calloc!(rt_pow2_1bytes_4align_calloc, 1, 4);
rt_mallocx!(rt_pow2_1bytes_4align_mallocx, 1, 4);
rt_mallocx_zeroed!(rt_pow2_1bytes_4align_mallocx_zeroed, 1, 4);
rt_mallocx_nallocx!(rt_pow2_1bytes_4align_mallocx_nallocx, 1, 4);
rt_alloc_layout_checked!(rt_pow2_1bytes_4align_alloc_layout_checked, 1, 4);
rt_alloc_layout_unchecked!(rt_pow2_1bytes_4align_alloc_layout_unchecked, 1, 4);
rt_alloc_excess_unused!(rt_pow2_1bytes_4align_alloc_excess_unused, 1, 4);
rt_alloc_excess_used!(rt_pow2_1bytes_4align_alloc_excess_used, 1, 4);
rt_realloc_naive!(rt_pow2_1bytes_4align_realloc_naive, 1, 4);
rt_realloc!(rt_pow2_1bytes_4align_realloc, 1, 4);
rt_realloc_excess_unused!(rt_pow2_1bytes_4align_realloc_excess_unused, 1, 4);
rt_realloc_excess_used!(rt_pow2_1bytes_4align_realloc_excess_used, 1, 4);

rt_calloc!(rt_pow2_2bytes_4align_calloc, 2, 4);
rt_mallocx!(rt_pow2_2bytes_4align_mallocx, 2, 4);
rt_mallocx_zeroed!(rt_pow2_2bytes_4align_mallocx_zeroed, 2, 4);
rt_mallocx_nallocx!(rt_pow2_2bytes_4align_mallocx_nallocx, 2, 4);
rt_alloc_layout_checked!(rt_pow2_2bytes_4align_alloc_layout_checked, 2, 4);
rt_alloc_layout_unchecked!(rt_pow2_2bytes_4align_alloc_layout_unchecked, 2, 4);
rt_alloc_excess_unused!(rt_pow2_2bytes_4align_alloc_excess_unused, 2, 4);
rt_alloc_excess_used!(rt_pow2_2bytes_4align_alloc_excess_used, 2, 4);
rt_realloc_naive!(rt_pow2_2bytes_4align_realloc_naive, 2, 4);
rt_realloc!(rt_pow2_2bytes_4align_realloc, 2, 4);
rt_realloc_excess_unused!(rt_pow2_2bytes_4align_realloc_excess_unused, 2, 4);
rt_realloc_excess_used!(rt_pow2_2bytes_4align_realloc_excess_used, 2, 4);

rt_calloc!(rt_pow2_4bytes_4align_calloc, 4, 4);
rt_mallocx!(rt_pow2_4bytes_4align_mallocx, 4, 4);
rt_mallocx_zeroed!(rt_pow2_4bytes_4align_mallocx_zeroed, 4, 4);
rt_mallocx_nallocx!(rt_pow2_4bytes_4align_mallocx_nallocx, 4, 4);
rt_alloc_layout_checked!(rt_pow2_4bytes_4align_alloc_layout_checked, 4, 4);
rt_alloc_layout_unchecked!(rt_pow2_4bytes_4align_alloc_layout_unchecked, 4, 4);
rt_alloc_excess_unused!(rt_pow2_4bytes_4align_alloc_excess_unused, 4, 4);
rt_alloc_excess_used!(rt_pow2_4bytes_4align_alloc_excess_used, 4, 4);
rt_realloc_naive!(rt_pow2_4bytes_4align_realloc_naive, 4, 4);
rt_realloc!(rt_pow2_4bytes_4align_realloc, 4, 4);
rt_realloc_excess_unused!(rt_pow2_4bytes_4align_realloc_excess_unused, 4, 4);
rt_realloc_excess_used!(rt_pow2_4bytes_4align_realloc_excess_used, 4, 4);

rt_calloc!(rt_pow2_8bytes_4align_calloc, 8, 4);
rt_mallocx!(rt_pow2_8bytes_4align_mallocx, 8, 4);
rt_mallocx_zeroed!(rt_pow2_8bytes_4align_mallocx_zeroed, 8, 4);
rt_mallocx_nallocx!(rt_pow2_8bytes_4align_mallocx_nallocx, 8, 4);
rt_alloc_layout_checked!(rt_pow2_8bytes_4align_alloc_layout_checked, 8, 4);
rt_alloc_layout_unchecked!(rt_pow2_8bytes_4align_alloc_layout_unchecked, 8, 4);
rt_alloc_excess_unused!(rt_pow2_8bytes_4align_alloc_excess_unused, 8, 4);
rt_alloc_excess_used!(rt_pow2_8bytes_4align_alloc_excess_used, 8, 4);
rt_realloc_naive!(rt_pow2_8bytes_4align_realloc_naive, 8, 4);
rt_realloc!(rt_pow2_8bytes_4align_realloc, 8, 4);
rt_realloc_excess_unused!(rt_pow2_8bytes_4align_realloc_excess_unused, 8, 4);
rt_realloc_excess_used!(rt_pow2_8bytes_4align_realloc_excess_used, 8, 4);

rt_calloc!(rt_pow2_16bytes_4align_calloc, 16, 4);
rt_mallocx!(rt_pow2_16bytes_4align_mallocx, 16, 4);
rt_mallocx_zeroed!(rt_pow2_16bytes_4align_mallocx_zeroed, 16, 4);
rt_mallocx_nallocx!(rt_pow2_16bytes_4align_mallocx_nallocx, 16, 4);
rt_alloc_layout_checked!(rt_pow2_16bytes_4align_alloc_layout_checked, 16, 4);
rt_alloc_layout_unchecked!(rt_pow2_16bytes_4align_alloc_layout_unchecked, 16, 4);
rt_alloc_excess_unused!(rt_pow2_16bytes_4align_alloc_excess_unused, 16, 4);
rt_alloc_excess_used!(rt_pow2_16bytes_4align_alloc_excess_used, 16, 4);
rt_realloc_naive!(rt_pow2_16bytes_4align_realloc_naive, 16, 4);
rt_realloc!(rt_pow2_16bytes_4align_realloc, 16, 4);
rt_realloc_excess_unused!(rt_pow2_16bytes_4align_realloc_excess_unused, 16, 4);
rt_realloc_excess_used!(rt_pow2_16bytes_4align_realloc_excess_used, 16, 4);

rt_calloc!(rt_pow2_32bytes_4align_calloc, 32, 4);
rt_mallocx!(rt_pow2_32bytes_4align_mallocx, 32, 4);
rt_mallocx_zeroed!(rt_pow2_32bytes_4align_mallocx_zeroed, 32, 4);
rt_mallocx_nallocx!(rt_pow2_32bytes_4align_mallocx_nallocx, 32, 4);
rt_alloc_layout_checked!(rt_pow2_32bytes_4align_alloc_layout_checked, 32, 4);
rt_alloc_layout_unchecked!(rt_pow2_32bytes_4align_alloc_layout_unchecked, 32, 4);
rt_alloc_excess_unused!(rt_pow2_32bytes_4align_alloc_excess_unused, 32, 4);
rt_alloc_excess_used!(rt_pow2_32bytes_4align_alloc_excess_used, 32, 4);
rt_realloc_naive!(rt_pow2_32bytes_4align_realloc_naive, 32, 4);
rt_realloc!(rt_pow2_32bytes_4align_realloc, 32, 4);
rt_realloc_excess_unused!(rt_pow2_32bytes_4align_realloc_excess_unused, 32, 4);
rt_realloc_excess_used!(rt_pow2_32bytes_4align_realloc_excess_used, 32, 4);

rt_calloc!(rt_pow2_64bytes_4align_calloc, 64, 4);
rt_mallocx!(rt_pow2_64bytes_4align_mallocx, 64, 4);
rt_mallocx_zeroed!(rt_pow2_64bytes_4align_mallocx_zeroed, 64, 4);
rt_mallocx_nallocx!(rt_pow2_64bytes_4align_mallocx_nallocx, 64, 4);
rt_alloc_layout_checked!(rt_pow2_64bytes_4align_alloc_layout_checked, 64, 4);
rt_alloc_layout_unchecked!(rt_pow2_64bytes_4align_alloc_layout_unchecked, 64, 4);
rt_alloc_excess_unused!(rt_pow2_64bytes_4align_alloc_excess_unused, 64, 4);
rt_alloc_excess_used!(rt_pow2_64bytes_4align_alloc_excess_used, 64, 4);
rt_realloc_naive!(rt_pow2_64bytes_4align_realloc_naive, 64, 4);
rt_realloc!(rt_pow2_64bytes_4align_realloc, 64, 4);
rt_realloc_excess_unused!(rt_pow2_64bytes_4align_realloc_excess_unused, 64, 4);
rt_realloc_excess_used!(rt_pow2_64bytes_4align_realloc_excess_used, 64, 4);

rt_calloc!(rt_pow2_128bytes_4align_calloc, 128, 4);
rt_mallocx!(rt_pow2_128bytes_4align_mallocx, 128, 4);
rt_mallocx_zeroed!(rt_pow2_128bytes_4align_mallocx_zeroed, 128, 4);
rt_mallocx_nallocx!(rt_pow2_128bytes_4align_mallocx_nallocx, 128, 4);
rt_alloc_layout_checked!(rt_pow2_128bytes_4align_alloc_layout_checked, 128, 4);
rt_alloc_layout_unchecked!(rt_pow2_128bytes_4align_alloc_layout_unchecked, 128, 4);
rt_alloc_excess_unused!(rt_pow2_128bytes_4align_alloc_excess_unused, 128, 4);
rt_alloc_excess_used!(rt_pow2_128bytes_4align_alloc_excess_used, 128, 4);
rt_realloc_naive!(rt_pow2_128bytes_4align_realloc_naive, 128, 4);
rt_realloc!(rt_pow2_128bytes_4align_realloc, 128, 4);
rt_realloc_excess_unused!(rt_pow2_128bytes_4align_realloc_excess_unused, 128, 4);
rt_realloc_excess_used!(rt_pow2_128bytes_4align_realloc_excess_used, 128, 4);

rt_calloc!(rt_pow2_256bytes_4align_calloc, 256, 4);
rt_mallocx!(rt_pow2_256bytes_4align_mallocx, 256, 4);
rt_mallocx_zeroed!(rt_pow2_256bytes_4align_mallocx_zeroed, 256, 4);
rt_mallocx_nallocx!(rt_pow2_256bytes_4align_mallocx_nallocx, 256, 4);
rt_alloc_layout_checked!(rt_pow2_256bytes_4align_alloc_layout_checked, 256, 4);
rt_alloc_layout_unchecked!(rt_pow2_256bytes_4align_alloc_layout_unchecked, 256, 4);
rt_alloc_excess_unused!(rt_pow2_256bytes_4align_alloc_excess_unused, 256, 4);
rt_alloc_excess_used!(rt_pow2_256bytes_4align_alloc_excess_used, 256, 4);
rt_realloc_naive!(rt_pow2_256bytes_4align_realloc_naive, 256, 4);
rt_realloc!(rt_pow2_256bytes_4align_realloc, 256, 4);
rt_realloc_excess_unused!(rt_pow2_256bytes_4align_realloc_excess_unused, 256, 4);
rt_realloc_excess_used!(rt_pow2_256bytes_4align_realloc_excess_used, 256, 4);

rt_calloc!(rt_pow2_512bytes_4align_calloc, 512, 4);
rt_mallocx!(rt_pow2_512bytes_4align_mallocx, 512, 4);
rt_mallocx_zeroed!(rt_pow2_512bytes_4align_mallocx_zeroed, 512, 4);
rt_mallocx_nallocx!(rt_pow2_512bytes_4align_mallocx_nallocx, 512, 4);
rt_alloc_layout_checked!(rt_pow2_512bytes_4align_alloc_layout_checked, 512, 4);
rt_alloc_layout_unchecked!(rt_pow2_512bytes_4align_alloc_layout_unchecked, 512, 4);
rt_alloc_excess_unused!(rt_pow2_512bytes_4align_alloc_excess_unused, 512, 4);
rt_alloc_excess_used!(rt_pow2_512bytes_4align_alloc_excess_used, 512, 4);
rt_realloc_naive!(rt_pow2_512bytes_4align_realloc_naive, 512, 4);
rt_realloc!(rt_pow2_512bytes_4align_realloc, 512, 4);
rt_realloc_excess_unused!(rt_pow2_512bytes_4align_realloc_excess_unused, 512, 4);
rt_realloc_excess_used!(rt_pow2_512bytes_4align_realloc_excess_used, 512, 4);

rt_calloc!(rt_pow2_1024bytes_4align_calloc, 1024, 4);
rt_mallocx!(rt_pow2_1024bytes_4align_mallocx, 1024, 4);
rt_mallocx_zeroed!(rt_pow2_1024bytes_4align_mallocx_zeroed, 1024, 4);
rt_mallocx_nallocx!(rt_pow2_1024bytes_4align_mallocx_nallocx, 1024, 4);
rt_alloc_layout_checked!(rt_pow2_1024bytes_4align_alloc_layout_checked, 1024, 4);
rt_alloc_layout_unchecked!(rt_pow2_1024bytes_4align_alloc_layout_unchecked, 1024, 4);
rt_alloc_excess_unused!(rt_pow2_1024bytes_4align_alloc_excess_unused, 1024, 4);
rt_alloc_excess_used!(rt_pow2_1024bytes_4align_alloc_excess_used, 1024, 4);
rt_realloc_naive!(rt_pow2_1024bytes_4align_realloc_naive, 1024, 4);
rt_realloc!(rt_pow2_1024bytes_4align_realloc, 1024, 4);
rt_realloc_excess_unused!(rt_pow2_1024bytes_4align_realloc_excess_unused, 1024, 4);
rt_realloc_excess_used!(rt_pow2_1024bytes_4align_realloc_excess_used, 1024, 4);

rt_calloc!(rt_pow2_2048bytes_4align_calloc, 2048, 4);
rt_mallocx!(rt_pow2_2048bytes_4align_mallocx, 2048, 4);
rt_mallocx_zeroed!(rt_pow2_2048bytes_4align_mallocx_zeroed, 2048, 4);
rt_mallocx_nallocx!(rt_pow2_2048bytes_4align_mallocx_nallocx, 2048, 4);
rt_alloc_layout_checked!(rt_pow2_2048bytes_4align_alloc_layout_checked, 2048, 4);
rt_alloc_layout_unchecked!(rt_pow2_2048bytes_4align_alloc_layout_unchecked, 2048, 4);
rt_alloc_excess_unused!(rt_pow2_2048bytes_4align_alloc_excess_unused, 2048, 4);
rt_alloc_excess_used!(rt_pow2_2048bytes_4align_alloc_excess_used, 2048, 4);
rt_realloc_naive!(rt_pow2_2048bytes_4align_realloc_naive, 2048, 4);
rt_realloc!(rt_pow2_2048bytes_4align_realloc, 2048, 4);
rt_realloc_excess_unused!(rt_pow2_2048bytes_4align_realloc_excess_unused, 2048, 4);
rt_realloc_excess_used!(rt_pow2_2048bytes_4align_realloc_excess_used, 2048, 4);

rt_calloc!(rt_pow2_4096bytes_4align_calloc, 4096, 4);
rt_mallocx!(rt_pow2_4096bytes_4align_mallocx, 4096, 4);
rt_mallocx_zeroed!(rt_pow2_4096bytes_4align_mallocx_zeroed, 4096, 4);
rt_mallocx_nallocx!(rt_pow2_4096bytes_4align_mallocx_nallocx, 4096, 4);
rt_alloc_layout_checked!(rt_pow2_4096bytes_4align_alloc_layout_checked, 4096, 4);
rt_alloc_layout_unchecked!(rt_pow2_4096bytes_4align_alloc_layout_unchecked, 4096, 4);
rt_alloc_excess_unused!(rt_pow2_4096bytes_4align_alloc_excess_unused, 4096, 4);
rt_alloc_excess_used!(rt_pow2_4096bytes_4align_alloc_excess_used, 4096, 4);
rt_realloc_naive!(rt_pow2_4096bytes_4align_realloc_naive, 4096, 4);
rt_realloc!(rt_pow2_4096bytes_4align_realloc, 4096, 4);
rt_realloc_excess_unused!(rt_pow2_4096bytes_4align_realloc_excess_unused, 4096, 4);
rt_realloc_excess_used!(rt_pow2_4096bytes_4align_realloc_excess_used, 4096, 4);

rt_calloc!(rt_pow2_8192bytes_4align_calloc, 8192, 4);
rt_mallocx!(rt_pow2_8192bytes_4align_mallocx, 8192, 4);
rt_mallocx_zeroed!(rt_pow2_8192bytes_4align_mallocx_zeroed, 8192, 4);
rt_mallocx_nallocx!(rt_pow2_8192bytes_4align_mallocx_nallocx, 8192, 4);
rt_alloc_layout_checked!(rt_pow2_8192bytes_4align_alloc_layout_checked, 8192, 4);
rt_alloc_layout_unchecked!(rt_pow2_8192bytes_4align_alloc_layout_unchecked, 8192, 4);
rt_alloc_excess_unused!(rt_pow2_8192bytes_4align_alloc_excess_unused, 8192, 4);
rt_alloc_excess_used!(rt_pow2_8192bytes_4align_alloc_excess_used, 8192, 4);
rt_realloc_naive!(rt_pow2_8192bytes_4align_realloc_naive, 8192, 4);
rt_realloc!(rt_pow2_8192bytes_4align_realloc, 8192, 4);
rt_realloc_excess_unused!(rt_pow2_8192bytes_4align_realloc_excess_unused, 8192, 4);
rt_realloc_excess_used!(rt_pow2_8192bytes_4align_realloc_excess_used, 8192, 4);

rt_calloc!(rt_pow2_16384bytes_4align_calloc, 16384, 4);
rt_mallocx!(rt_pow2_16384bytes_4align_mallocx, 16384, 4);
rt_mallocx_zeroed!(rt_pow2_16384bytes_4align_mallocx_zeroed, 16384, 4);
rt_mallocx_nallocx!(rt_pow2_16384bytes_4align_mallocx_nallocx, 16384, 4);
rt_alloc_layout_checked!(rt_pow2_16384bytes_4align_alloc_layout_checked, 16384, 4);
rt_alloc_layout_unchecked!(rt_pow2_16384bytes_4align_alloc_layout_unchecked, 16384, 4);
rt_alloc_excess_unused!(rt_pow2_16384bytes_4align_alloc_excess_unused, 16384, 4);
rt_alloc_excess_used!(rt_pow2_16384bytes_4align_alloc_excess_used, 16384, 4);
rt_realloc_naive!(rt_pow2_16384bytes_4align_realloc_naive, 16384, 4);
rt_realloc!(rt_pow2_16384bytes_4align_realloc, 16384, 4);
rt_realloc_excess_unused!(rt_pow2_16384bytes_4align_realloc_excess_unused, 16384, 4);
rt_realloc_excess_used!(rt_pow2_16384bytes_4align_realloc_excess_used, 16384, 4);

rt_calloc!(rt_pow2_32768bytes_4align_calloc, 32768, 4);
rt_mallocx!(rt_pow2_32768bytes_4align_mallocx, 32768, 4);
rt_mallocx_zeroed!(rt_pow2_32768bytes_4align_mallocx_zeroed, 32768, 4);
rt_mallocx_nallocx!(rt_pow2_32768bytes_4align_mallocx_nallocx, 32768, 4);
rt_alloc_layout_checked!(rt_pow2_32768bytes_4align_alloc_layout_checked, 32768, 4);
rt_alloc_layout_unchecked!(rt_pow2_32768bytes_4align_alloc_layout_unchecked, 32768, 4);
rt_alloc_excess_unused!(rt_pow2_32768bytes_4align_alloc_excess_unused, 32768, 4);
rt_alloc_excess_used!(rt_pow2_32768bytes_4align_alloc_excess_used, 32768, 4);
rt_realloc_naive!(rt_pow2_32768bytes_4align_realloc_naive, 32768, 4);
rt_realloc!(rt_pow2_32768bytes_4align_realloc, 32768, 4);
rt_realloc_excess_unused!(rt_pow2_32768bytes_4align_realloc_excess_unused, 32768, 4);
rt_realloc_excess_used!(rt_pow2_32768bytes_4align_realloc_excess_used, 32768, 4);

rt_calloc!(rt_pow2_65536bytes_4align_calloc, 65536, 4);
rt_mallocx!(rt_pow2_65536bytes_4align_mallocx, 65536, 4);
rt_mallocx_zeroed!(rt_pow2_65536bytes_4align_mallocx_zeroed, 65536, 4);
rt_mallocx_nallocx!(rt_pow2_65536bytes_4align_mallocx_nallocx, 65536, 4);
rt_alloc_layout_checked!(rt_pow2_65536bytes_4align_alloc_layout_checked, 65536, 4);
rt_alloc_layout_unchecked!(rt_pow2_65536bytes_4align_alloc_layout_unchecked, 65536, 4);
rt_alloc_excess_unused!(rt_pow2_65536bytes_4align_alloc_excess_unused, 65536, 4);
rt_alloc_excess_used!(rt_pow2_65536bytes_4align_alloc_excess_used, 65536, 4);
rt_realloc_naive!(rt_pow2_65536bytes_4align_realloc_naive, 65536, 4);
rt_realloc!(rt_pow2_65536bytes_4align_realloc, 65536, 4);
rt_realloc_excess_unused!(rt_pow2_65536bytes_4align_realloc_excess_unused, 65536, 4);
rt_realloc_excess_used!(rt_pow2_65536bytes_4align_realloc_excess_used, 65536, 4);

rt_calloc!(rt_pow2_131072bytes_4align_calloc, 131072, 4);
rt_mallocx!(rt_pow2_131072bytes_4align_mallocx, 131072, 4);
rt_mallocx_zeroed!(rt_pow2_131072bytes_4align_mallocx_zeroed, 131072, 4);
rt_mallocx_nallocx!(rt_pow2_131072bytes_4align_mallocx_nallocx, 131072, 4);
rt_alloc_layout_checked!(rt_pow2_131072bytes_4align_alloc_layout_checked, 131072, 4);
rt_alloc_layout_unchecked!(rt_pow2_131072bytes_4align_alloc_layout_unchecked, 131072, 4);
rt_alloc_excess_unused!(rt_pow2_131072bytes_4align_alloc_excess_unused, 131072, 4);
rt_alloc_excess_used!(rt_pow2_131072bytes_4align_alloc_excess_used, 131072, 4);
rt_realloc_naive!(rt_pow2_131072bytes_4align_realloc_naive, 131072, 4);
rt_realloc!(rt_pow2_131072bytes_4align_realloc, 131072, 4);
rt_realloc_excess_unused!(rt_pow2_131072bytes_4align_realloc_excess_unused, 131072, 4);
rt_realloc_excess_used!(rt_pow2_131072bytes_4align_realloc_excess_used, 131072, 4);

rt_calloc!(rt_pow2_4194304bytes_4align_calloc, 4194304, 4);
rt_mallocx!(rt_pow2_4194304bytes_4align_mallocx, 4194304, 4);
rt_mallocx_zeroed!(rt_pow2_4194304bytes_4align_mallocx_zeroed, 4194304, 4);
rt_mallocx_nallocx!(rt_pow2_4194304bytes_4align_mallocx_nallocx, 4194304, 4);
rt_alloc_layout_checked!(rt_pow2_4194304bytes_4align_alloc_layout_checked, 4194304, 4);
rt_alloc_layout_unchecked!(
    rt_pow2_4194304bytes_4align_alloc_layout_unchecked,
    4194304,
    4
);
rt_alloc_excess_unused!(rt_pow2_4194304bytes_4align_alloc_excess_unused, 4194304, 4);
rt_alloc_excess_used!(rt_pow2_4194304bytes_4align_alloc_excess_used, 4194304, 4);
rt_realloc_naive!(rt_pow2_4194304bytes_4align_realloc_naive, 4194304, 4);
rt_realloc!(rt_pow2_4194304bytes_4align_realloc, 4194304, 4);
rt_realloc_excess_unused!(
    rt_pow2_4194304bytes_4align_realloc_excess_unused,
    4194304,
    4
);
rt_realloc_excess_used!(rt_pow2_4194304bytes_4align_realloc_excess_used, 4194304, 4);

// Even
rt_calloc!(rt_even_10bytes_4align_calloc, 10, 4);
rt_mallocx!(rt_even_10bytes_4align_mallocx, 10, 4);
rt_mallocx_zeroed!(rt_even_10bytes_4align_mallocx_zeroed, 10, 4);
rt_mallocx_nallocx!(rt_even_10bytes_4align_mallocx_nallocx, 10, 4);
rt_alloc_layout_checked!(rt_even_10bytes_4align_alloc_layout_checked, 10, 4);
rt_alloc_layout_unchecked!(rt_even_10bytes_4align_alloc_layout_unchecked, 10, 4);
rt_alloc_excess_unused!(rt_even_10bytes_4align_alloc_excess_unused, 10, 4);
rt_alloc_excess_used!(rt_even_10bytes_4align_alloc_excess_used, 10, 4);
rt_realloc_naive!(rt_even_10bytes_4align_realloc_naive, 10, 4);
rt_realloc!(rt_even_10bytes_4align_realloc, 10, 4);
rt_realloc_excess_unused!(rt_even_10bytes_4align_realloc_excess_unused, 10, 4);
rt_realloc_excess_used!(rt_even_10bytes_4align_realloc_excess_used, 10, 4);

rt_calloc!(rt_even_100bytes_4align_calloc, 100, 4);
rt_mallocx!(rt_even_100bytes_4align_mallocx, 100, 4);
rt_mallocx_zeroed!(rt_even_100bytes_4align_mallocx_zeroed, 100, 4);
rt_mallocx_nallocx!(rt_even_100bytes_4align_mallocx_nallocx, 100, 4);
rt_alloc_layout_checked!(rt_even_100bytes_4align_alloc_layout_checked, 100, 4);
rt_alloc_layout_unchecked!(rt_even_100bytes_4align_alloc_layout_unchecked, 100, 4);
rt_alloc_excess_unused!(rt_even_100bytes_4align_alloc_excess_unused, 100, 4);
rt_alloc_excess_used!(rt_even_100bytes_4align_alloc_excess_used, 100, 4);
rt_realloc_naive!(rt_even_100bytes_4align_realloc_naive, 100, 4);
rt_realloc!(rt_even_100bytes_4align_realloc, 100, 4);
rt_realloc_excess_unused!(rt_even_100bytes_4align_realloc_excess_unused, 100, 4);
rt_realloc_excess_used!(rt_even_100bytes_4align_realloc_excess_used, 100, 4);

rt_calloc!(rt_even_1000bytes_4align_calloc, 1000, 4);
rt_mallocx!(rt_even_1000bytes_4align_mallocx, 1000, 4);
rt_mallocx_zeroed!(rt_even_1000bytes_4align_mallocx_zeroed, 1000, 4);
rt_mallocx_nallocx!(rt_even_1000bytes_4align_mallocx_nallocx, 1000, 4);
rt_alloc_layout_checked!(rt_even_1000bytes_4align_alloc_layout_checked, 1000, 4);
rt_alloc_layout_unchecked!(rt_even_1000bytes_4align_alloc_layout_unchecked, 1000, 4);
rt_alloc_excess_unused!(rt_even_1000bytes_4align_alloc_excess_unused, 1000, 4);
rt_alloc_excess_used!(rt_even_1000bytes_4align_alloc_excess_used, 1000, 4);
rt_realloc_naive!(rt_even_1000bytes_4align_realloc_naive, 1000, 4);
rt_realloc!(rt_even_1000bytes_4align_realloc, 1000, 4);
rt_realloc_excess_unused!(rt_even_1000bytes_4align_realloc_excess_unused, 1000, 4);
rt_realloc_excess_used!(rt_even_1000bytes_4align_realloc_excess_used, 1000, 4);

rt_calloc!(rt_even_10000bytes_4align_calloc, 10000, 4);
rt_mallocx!(rt_even_10000bytes_4align_mallocx, 10000, 4);
rt_mallocx_zeroed!(rt_even_10000bytes_4align_mallocx_zeroed, 10000, 4);
rt_mallocx_nallocx!(rt_even_10000bytes_4align_mallocx_nallocx, 10000, 4);
rt_alloc_layout_checked!(rt_even_10000bytes_4align_alloc_layout_checked, 10000, 4);
rt_alloc_layout_unchecked!(rt_even_10000bytes_4align_alloc_layout_unchecked, 10000, 4);
rt_alloc_excess_unused!(rt_even_10000bytes_4align_alloc_excess_unused, 10000, 4);
rt_alloc_excess_used!(rt_even_10000bytes_4align_alloc_excess_used, 10000, 4);
rt_realloc_naive!(rt_even_10000bytes_4align_realloc_naive, 10000, 4);
rt_realloc!(rt_even_10000bytes_4align_realloc, 10000, 4);
rt_realloc_excess_unused!(rt_even_10000bytes_4align_realloc_excess_unused, 10000, 4);
rt_realloc_excess_used!(rt_even_10000bytes_4align_realloc_excess_used, 10000, 4);

rt_calloc!(rt_even_100000bytes_4align_calloc, 100000, 4);
rt_mallocx!(rt_even_100000bytes_4align_mallocx, 100000, 4);
rt_mallocx_zeroed!(rt_even_100000bytes_4align_mallocx_zeroed, 100000, 4);
rt_mallocx_nallocx!(rt_even_100000bytes_4align_mallocx_nallocx, 100000, 4);
rt_alloc_layout_checked!(rt_even_100000bytes_4align_alloc_layout_checked, 100000, 4);
rt_alloc_layout_unchecked!(rt_even_100000bytes_4align_alloc_layout_unchecked, 100000, 4);
rt_alloc_excess_unused!(rt_even_100000bytes_4align_alloc_excess_unused, 100000, 4);
rt_alloc_excess_used!(rt_even_100000bytes_4align_alloc_excess_used, 100000, 4);
rt_realloc_naive!(rt_even_100000bytes_4align_realloc_naive, 100000, 4);
rt_realloc!(rt_even_100000bytes_4align_realloc, 100000, 4);
rt_realloc_excess_unused!(rt_even_100000bytes_4align_realloc_excess_unused, 100000, 4);
rt_realloc_excess_used!(rt_even_100000bytes_4align_realloc_excess_used, 100000, 4);

rt_calloc!(rt_even_1000000bytes_4align_calloc, 1000000, 4);
rt_mallocx!(rt_even_1000000bytes_4align_mallocx, 1000000, 4);
rt_mallocx_zeroed!(rt_even_1000000bytes_4align_mallocx_zeroed, 1000000, 4);
rt_mallocx_nallocx!(rt_even_1000000bytes_4align_mallocx_nallocx, 1000000, 4);
rt_alloc_layout_checked!(rt_even_1000000bytes_4align_alloc_layout_checked, 1000000, 4);
rt_alloc_layout_unchecked!(
    rt_even_1000000bytes_4align_alloc_layout_unchecked,
    1000000,
    4
);
rt_alloc_excess_unused!(rt_even_1000000bytes_4align_alloc_excess_unused, 1000000, 4);
rt_alloc_excess_used!(rt_even_1000000bytes_4align_alloc_excess_used, 1000000, 4);
rt_realloc_naive!(rt_even_1000000bytes_4align_realloc_naive, 1000000, 4);
rt_realloc!(rt_even_1000000bytes_4align_realloc, 1000000, 4);
rt_realloc_excess_unused!(
    rt_even_1000000bytes_4align_realloc_excess_unused,
    1000000,
    4
);
rt_realloc_excess_used!(rt_even_1000000bytes_4align_realloc_excess_used, 1000000, 4);

// Odd:
rt_calloc!(rt_odd_10bytes_4align_calloc, 10 - 1, 4);
rt_mallocx!(rt_odd_10bytes_4align_mallocx, 10 - 1, 4);
rt_mallocx_zeroed!(rt_odd_10bytes_4align_mallocx_zeroed, 10 - 1, 4);
rt_mallocx_nallocx!(rt_odd_10bytes_4align_mallocx_nallocx, 10 - 1, 4);
rt_alloc_layout_checked!(rt_odd_10bytes_4align_alloc_layout_checked, 10 - 1, 4);
rt_alloc_layout_unchecked!(rt_odd_10bytes_4align_alloc_layout_unchecked, 10 - 1, 4);
rt_alloc_excess_unused!(rt_odd_10bytes_4align_alloc_excess_unused, 10 - 1, 4);
rt_alloc_excess_used!(rt_odd_10bytes_4align_alloc_excess_used, 10 - 1, 4);
rt_realloc_naive!(rt_odd_10bytes_4align_realloc_naive, 10 - 1, 4);
rt_realloc!(rt_odd_10bytes_4align_realloc, 10 - 1, 4);
rt_realloc_excess_unused!(rt_odd_10bytes_4align_realloc_excess_unused, 10 - 1, 4);
rt_realloc_excess_used!(rt_odd_10bytes_4align_realloc_excess_used, 10 - 1, 4);

rt_calloc!(rt_odd_100bytes_4align_calloc, 100 - 1, 4);
rt_mallocx!(rt_odd_100bytes_4align_mallocx, 100 - 1, 4);
rt_mallocx_zeroed!(rt_odd_100bytes_4align_mallocx_zeroed, 100 - 1, 4);
rt_mallocx_nallocx!(rt_odd_100bytes_4align_mallocx_nallocx, 100 - 1, 4);
rt_alloc_layout_checked!(rt_odd_100bytes_4align_alloc_layout_checked, 100 - 1, 4);
rt_alloc_layout_unchecked!(rt_odd_100bytes_4align_alloc_layout_unchecked, 100 - 1, 4);
rt_alloc_excess_unused!(rt_odd_100bytes_4align_alloc_excess_unused, 100 - 1, 4);
rt_alloc_excess_used!(rt_odd_100bytes_4align_alloc_excess_used, 100 - 1, 4);
rt_realloc_naive!(rt_odd_100bytes_4align_realloc_naive, 100 - 1, 4);
rt_realloc!(rt_odd_100bytes_4align_realloc, 100 - 1, 4);
rt_realloc_excess_unused!(rt_odd_100bytes_4align_realloc_excess_unused, 100 - 1, 4);
rt_realloc_excess_used!(rt_odd_100bytes_4align_realloc_excess_used, 100 - 1, 4);

rt_calloc!(rt_odd_1000bytes_4align_calloc, 1000 - 1, 4);
rt_mallocx!(rt_odd_1000bytes_4align_mallocx, 1000 - 1, 4);
rt_mallocx_zeroed!(rt_odd_1000bytes_4align_mallocx_zeroed, 1000 - 1, 4);
rt_mallocx_nallocx!(rt_odd_1000bytes_4align_mallocx_nallocx, 1000 - 1, 4);
rt_alloc_layout_checked!(rt_odd_1000bytes_4align_alloc_layout_checked, 1000 - 1, 4);
rt_alloc_layout_unchecked!(rt_odd_1000bytes_4align_alloc_layout_unchecked, 1000 - 1, 4);
rt_alloc_excess_unused!(rt_odd_1000bytes_4align_alloc_excess_unused, 1000 - 1, 4);
rt_alloc_excess_used!(rt_odd_1000bytes_4align_alloc_excess_used, 1000 - 1, 4);
rt_realloc_naive!(rt_odd_1000bytes_4align_realloc_naive, 1000 - 1, 4);
rt_realloc!(rt_odd_1000bytes_4align_realloc, 1000 - 1, 4);
rt_realloc_excess_unused!(rt_odd_1000bytes_4align_realloc_excess_unused, 1000 - 1, 4);
rt_realloc_excess_used!(rt_odd_1000bytes_4align_realloc_excess_used, 1000 - 1, 4);

rt_calloc!(rt_odd_10000bytes_4align_calloc, 10000 - 1, 4);
rt_mallocx!(rt_odd_10000bytes_4align_mallocx, 10000 - 1, 4);
rt_mallocx_zeroed!(rt_odd_10000bytes_4align_mallocx_zeroed, 10000 - 1, 4);
rt_mallocx_nallocx!(rt_odd_10000bytes_4align_mallocx_nallocx, 10000 - 1, 4);
rt_alloc_layout_checked!(rt_odd_10000bytes_4align_alloc_layout_checked, 10000 - 1, 4);
rt_alloc_layout_unchecked!(
    rt_odd_10000bytes_4align_alloc_layout_unchecked,
    10000 - 1,
    4
);
rt_alloc_excess_unused!(rt_odd_10000bytes_4align_alloc_excess_unused, 10000 - 1, 4);
rt_alloc_excess_used!(rt_odd_10000bytes_4align_alloc_excess_used, 10000 - 1, 4);
rt_realloc_naive!(rt_odd_10000bytes_4align_realloc_naive, 10000 - 1, 4);
rt_realloc!(rt_odd_10000bytes_4align_realloc, 10000 - 1, 4);
rt_realloc_excess_unused!(rt_odd_10000bytes_4align_realloc_excess_unused, 10000 - 1, 4);
rt_realloc_excess_used!(rt_odd_10000bytes_4align_realloc_excess_used, 10000 - 1, 4);

rt_calloc!(rt_odd_100000bytes_4align_calloc, 100000 - 1, 4);
rt_mallocx!(rt_odd_100000bytes_4align_mallocx, 100000 - 1, 4);
rt_mallocx_zeroed!(rt_odd_100000bytes_4align_mallocx_zeroed, 100000 - 1, 4);
rt_mallocx_nallocx!(rt_odd_100000bytes_4align_mallocx_nallocx, 100000 - 1, 4);
rt_alloc_layout_checked!(
    rt_odd_100000bytes_4align_alloc_layout_checked,
    100000 - 1,
    4
);
rt_alloc_layout_unchecked!(
    rt_odd_100000bytes_4align_alloc_layout_unchecked,
    100000 - 1,
    4
);
rt_alloc_excess_unused!(rt_odd_100000bytes_4align_alloc_excess_unused, 100000 - 1, 4);
rt_alloc_excess_used!(rt_odd_100000bytes_4align_alloc_excess_used, 100000 - 1, 4);
rt_realloc_naive!(rt_odd_100000bytes_4align_realloc_naive, 100000 - 1, 4);
rt_realloc!(rt_odd_100000bytes_4align_realloc, 100000 - 1, 4);
rt_realloc_excess_unused!(
    rt_odd_100000bytes_4align_realloc_excess_unused,
    100000 - 1,
    4
);
rt_realloc_excess_used!(rt_odd_100000bytes_4align_realloc_excess_used, 100000 - 1, 4);

rt_calloc!(rt_odd_1000000bytes_4align_calloc, 1000000 - 1, 4);
rt_mallocx!(rt_odd_1000000bytes_4align_mallocx, 1000000 - 1, 4);
rt_mallocx_zeroed!(rt_odd_1000000bytes_4align_mallocx_zeroed, 1000000 - 1, 4);
rt_mallocx_nallocx!(rt_odd_1000000bytes_4align_mallocx_nallocx, 1000000 - 1, 4);
rt_alloc_layout_checked!(
    rt_odd_1000000bytes_4align_alloc_layout_checked,
    1000000 - 1,
    4
);
rt_alloc_layout_unchecked!(
    rt_odd_1000000bytes_4align_alloc_layout_unchecked,
    1000000 - 1,
    4
);
rt_alloc_excess_unused!(
    rt_odd_1000000bytes_4align_alloc_excess_unused,
    1000000 - 1,
    4
);
rt_alloc_excess_used!(rt_odd_1000000bytes_4align_alloc_excess_used, 1000000 - 1, 4);
rt_realloc_naive!(rt_odd_1000000bytes_4align_realloc_naive, 1000000 - 1, 4);
rt_realloc!(rt_odd_1000000bytes_4align_realloc, 1000000 - 1, 4);
rt_realloc_excess_unused!(
    rt_odd_1000000bytes_4align_realloc_excess_unused,
    1000000 - 1,
    4
);
rt_realloc_excess_used!(
    rt_odd_1000000bytes_4align_realloc_excess_used,
    1000000 - 1,
    4
);

// primes
rt_calloc!(rt_primes_3bytes_4align_calloc, 3, 4);
rt_mallocx!(rt_primes_3bytes_4align_mallocx, 3, 4);
rt_mallocx_zeroed!(rt_primes_3bytes_4align_mallocx_zeroed, 3, 4);
rt_mallocx_nallocx!(rt_primes_3bytes_4align_mallocx_nallocx, 3, 4);
rt_alloc_layout_checked!(rt_primes_3bytes_4align_alloc_layout_checked, 3, 4);
rt_alloc_layout_unchecked!(rt_primes_3bytes_4align_alloc_layout_unchecked, 3, 4);
rt_alloc_excess_unused!(rt_primes_3bytes_4align_alloc_excess_unused, 3, 4);
rt_alloc_excess_used!(rt_primes_3bytes_4align_alloc_excess_used, 3, 4);
rt_realloc_naive!(rt_primes_3bytes_4align_realloc_naive, 3, 4);
rt_realloc!(rt_primes_3bytes_4align_realloc, 3, 4);
rt_realloc_excess_unused!(rt_primes_3bytes_4align_realloc_excess_unused, 3, 4);
rt_realloc_excess_used!(rt_primes_3bytes_4align_realloc_excess_used, 3, 4);

rt_calloc!(rt_primes_7bytes_4align_calloc, 7, 4);
rt_mallocx!(rt_primes_7bytes_4align_mallocx, 7, 4);
rt_mallocx_zeroed!(rt_primes_7bytes_4align_mallocx_zeroed, 7, 4);
rt_mallocx_nallocx!(rt_primes_7bytes_4align_mallocx_nallocx, 7, 4);
rt_alloc_layout_checked!(rt_primes_7bytes_4align_alloc_layout_checked, 7, 4);
rt_alloc_layout_unchecked!(rt_primes_7bytes_4align_alloc_layout_unchecked, 7, 4);
rt_alloc_excess_unused!(rt_primes_7bytes_4align_alloc_excess_unused, 7, 4);
rt_alloc_excess_used!(rt_primes_7bytes_4align_alloc_excess_used, 7, 4);
rt_realloc_naive!(rt_primes_7bytes_4align_realloc_naive, 7, 4);
rt_realloc!(rt_primes_7bytes_4align_realloc, 7, 4);
rt_realloc_excess_unused!(rt_primes_7bytes_4align_realloc_excess_unused, 7, 4);
rt_realloc_excess_used!(rt_primes_7bytes_4align_realloc_excess_used, 7, 4);

rt_calloc!(rt_primes_13bytes_4align_calloc, 13, 4);
rt_mallocx!(rt_primes_13bytes_4align_mallocx, 13, 4);
rt_mallocx_zeroed!(rt_primes_13bytes_4align_mallocx_zeroed, 13, 4);
rt_mallocx_nallocx!(rt_primes_13bytes_4align_mallocx_nallocx, 13, 4);
rt_alloc_layout_checked!(rt_primes_13bytes_4align_alloc_layout_checked, 13, 4);
rt_alloc_layout_unchecked!(rt_primes_13bytes_4align_alloc_layout_unchecked, 13, 4);
rt_alloc_excess_unused!(rt_primes_13bytes_4align_alloc_excess_unused, 13, 4);
rt_alloc_excess_used!(rt_primes_13bytes_4align_alloc_excess_used, 13, 4);
rt_realloc_naive!(rt_primes_13bytes_4align_realloc_naive, 13, 4);
rt_realloc!(rt_primes_13bytes_4align_realloc, 13, 4);
rt_realloc_excess_unused!(rt_primes_13bytes_4align_realloc_excess_unused, 13, 4);
rt_realloc_excess_used!(rt_primes_13bytes_4align_realloc_excess_used, 13, 4);

rt_calloc!(rt_primes_17bytes_4align_calloc, 17, 4);
rt_mallocx!(rt_primes_17bytes_4align_mallocx, 17, 4);
rt_mallocx_zeroed!(rt_primes_17bytes_4align_mallocx_zeroed, 17, 4);
rt_mallocx_nallocx!(rt_primes_17bytes_4align_mallocx_nallocx, 17, 4);
rt_alloc_layout_checked!(rt_primes_17bytes_4align_alloc_layout_checked, 17, 4);
rt_alloc_layout_unchecked!(rt_primes_17bytes_4align_alloc_layout_unchecked, 17, 4);
rt_alloc_excess_unused!(rt_primes_17bytes_4align_alloc_excess_unused, 17, 4);
rt_alloc_excess_used!(rt_primes_17bytes_4align_alloc_excess_used, 17, 4);
rt_realloc_naive!(rt_primes_17bytes_4align_realloc_naive, 17, 4);
rt_realloc!(rt_primes_17bytes_4align_realloc, 17, 4);
rt_realloc_excess_unused!(rt_primes_17bytes_4align_realloc_excess_unused, 17, 4);
rt_realloc_excess_used!(rt_primes_17bytes_4align_realloc_excess_used, 17, 4);

rt_calloc!(rt_primes_31bytes_4align_calloc, 31, 4);
rt_mallocx!(rt_primes_31bytes_4align_mallocx, 31, 4);
rt_mallocx_zeroed!(rt_primes_31bytes_4align_mallocx_zeroed, 31, 4);
rt_mallocx_nallocx!(rt_primes_31bytes_4align_mallocx_nallocx, 31, 4);
rt_alloc_layout_checked!(rt_primes_31bytes_4align_alloc_layout_checked, 31, 4);
rt_alloc_layout_unchecked!(rt_primes_31bytes_4align_alloc_layout_unchecked, 31, 4);
rt_alloc_excess_unused!(rt_primes_31bytes_4align_alloc_excess_unused, 31, 4);
rt_alloc_excess_used!(rt_primes_31bytes_4align_alloc_excess_used, 31, 4);
rt_realloc_naive!(rt_primes_31bytes_4align_realloc_naive, 31, 4);
rt_realloc!(rt_primes_31bytes_4align_realloc, 31, 4);
rt_realloc_excess_unused!(rt_primes_31bytes_4align_realloc_excess_unused, 31, 4);
rt_realloc_excess_used!(rt_primes_31bytes_4align_realloc_excess_used, 31, 4);

rt_calloc!(rt_primes_61bytes_4align_calloc, 61, 4);
rt_mallocx!(rt_primes_61bytes_4align_mallocx, 61, 4);
rt_mallocx_zeroed!(rt_primes_61bytes_4align_mallocx_zeroed, 61, 4);
rt_mallocx_nallocx!(rt_primes_61bytes_4align_mallocx_nallocx, 61, 4);
rt_alloc_layout_checked!(rt_primes_61bytes_4align_alloc_layout_checked, 61, 4);
rt_alloc_layout_unchecked!(rt_primes_61bytes_4align_alloc_layout_unchecked, 61, 4);
rt_alloc_excess_unused!(rt_primes_61bytes_4align_alloc_excess_unused, 61, 4);
rt_alloc_excess_used!(rt_primes_61bytes_4align_alloc_excess_used, 61, 4);
rt_realloc_naive!(rt_primes_61bytes_4align_realloc_naive, 61, 4);
rt_realloc!(rt_primes_61bytes_4align_realloc, 61, 4);
rt_realloc_excess_unused!(rt_primes_61bytes_4align_realloc_excess_unused, 61, 4);
rt_realloc_excess_used!(rt_primes_61bytes_4align_realloc_excess_used, 61, 4);

rt_calloc!(rt_primes_96bytes_4align_calloc, 96, 4);
rt_mallocx!(rt_primes_96bytes_4align_mallocx, 96, 4);
rt_mallocx_zeroed!(rt_primes_96bytes_4align_mallocx_zeroed, 96, 4);
rt_mallocx_nallocx!(rt_primes_96bytes_4align_mallocx_nallocx, 96, 4);
rt_alloc_layout_checked!(rt_primes_96bytes_4align_alloc_layout_checked, 96, 4);
rt_alloc_layout_unchecked!(rt_primes_96bytes_4align_alloc_layout_unchecked, 96, 4);
rt_alloc_excess_unused!(rt_primes_96bytes_4align_alloc_excess_unused, 96, 4);
rt_alloc_excess_used!(rt_primes_96bytes_4align_alloc_excess_used, 96, 4);
rt_realloc_naive!(rt_primes_96bytes_4align_realloc_naive, 96, 4);
rt_realloc!(rt_primes_96bytes_4align_realloc, 96, 4);
rt_realloc_excess_unused!(rt_primes_96bytes_4align_realloc_excess_unused, 96, 4);
rt_realloc_excess_used!(rt_primes_96bytes_4align_realloc_excess_used, 96, 4);

rt_calloc!(rt_primes_127bytes_4align_calloc, 127, 4);
rt_mallocx!(rt_primes_127bytes_4align_mallocx, 127, 4);
rt_mallocx_zeroed!(rt_primes_127bytes_4align_mallocx_zeroed, 127, 4);
rt_mallocx_nallocx!(rt_primes_127bytes_4align_mallocx_nallocx, 127, 4);
rt_alloc_layout_checked!(rt_primes_127bytes_4align_alloc_layout_checked, 127, 4);
rt_alloc_layout_unchecked!(rt_primes_127bytes_4align_alloc_layout_unchecked, 127, 4);
rt_alloc_excess_unused!(rt_primes_127bytes_4align_alloc_excess_unused, 127, 4);
rt_alloc_excess_used!(rt_primes_127bytes_4align_alloc_excess_used, 127, 4);
rt_realloc_naive!(rt_primes_127bytes_4align_realloc_naive, 127, 4);
rt_realloc!(rt_primes_127bytes_4align_realloc, 127, 4);
rt_realloc_excess_unused!(rt_primes_127bytes_4align_realloc_excess_unused, 127, 4);
rt_realloc_excess_used!(rt_primes_127bytes_4align_realloc_excess_used, 127, 4);

rt_calloc!(rt_primes_257bytes_4align_calloc, 257, 4);
rt_mallocx!(rt_primes_257bytes_4align_mallocx, 257, 4);
rt_mallocx_zeroed!(rt_primes_257bytes_4align_mallocx_zeroed, 257, 4);
rt_mallocx_nallocx!(rt_primes_257bytes_4align_mallocx_nallocx, 257, 4);
rt_alloc_layout_checked!(rt_primes_257bytes_4align_alloc_layout_checked, 257, 4);
rt_alloc_layout_unchecked!(rt_primes_257bytes_4align_alloc_layout_unchecked, 257, 4);
rt_alloc_excess_unused!(rt_primes_257bytes_4align_alloc_excess_unused, 257, 4);
rt_alloc_excess_used!(rt_primes_257bytes_4align_alloc_excess_used, 257, 4);
rt_realloc_naive!(rt_primes_257bytes_4align_realloc_naive, 257, 4);
rt_realloc!(rt_primes_257bytes_4align_realloc, 257, 4);
rt_realloc_excess_unused!(rt_primes_257bytes_4align_realloc_excess_unused, 257, 4);
rt_realloc_excess_used!(rt_primes_257bytes_4align_realloc_excess_used, 257, 4);

rt_calloc!(rt_primes_509bytes_4align_calloc, 509, 4);
rt_mallocx!(rt_primes_509bytes_4align_mallocx, 509, 4);
rt_mallocx_zeroed!(rt_primes_509bytes_4align_mallocx_zeroed, 509, 4);
rt_mallocx_nallocx!(rt_primes_509bytes_4align_mallocx_nallocx, 509, 4);
rt_alloc_layout_checked!(rt_primes_509bytes_4align_alloc_layout_checked, 509, 4);
rt_alloc_layout_unchecked!(rt_primes_509bytes_4align_alloc_layout_unchecked, 509, 4);
rt_alloc_excess_unused!(rt_primes_509bytes_4align_alloc_excess_unused, 509, 4);
rt_alloc_excess_used!(rt_primes_509bytes_4align_alloc_excess_used, 509, 4);
rt_realloc_naive!(rt_primes_509bytes_4align_realloc_naive, 509, 4);
rt_realloc!(rt_primes_509bytes_4align_realloc, 509, 4);
rt_realloc_excess_unused!(rt_primes_509bytes_4align_realloc_excess_unused, 509, 4);
rt_realloc_excess_used!(rt_primes_509bytes_4align_realloc_excess_used, 509, 4);

rt_calloc!(rt_primes_1021bytes_4align_calloc, 1021, 4);
rt_mallocx!(rt_primes_1021bytes_4align_mallocx, 1021, 4);
rt_mallocx_zeroed!(rt_primes_1021bytes_4align_mallocx_zeroed, 1021, 4);
rt_mallocx_nallocx!(rt_primes_1021bytes_4align_mallocx_nallocx, 1021, 4);
rt_alloc_layout_checked!(rt_primes_1021bytes_4align_alloc_layout_checked, 1021, 4);
rt_alloc_layout_unchecked!(rt_primes_1021bytes_4align_alloc_layout_unchecked, 1021, 4);
rt_alloc_excess_unused!(rt_primes_1021bytes_4align_alloc_excess_unused, 1021, 4);
rt_alloc_excess_used!(rt_primes_1021bytes_4align_alloc_excess_used, 1021, 4);
rt_realloc_naive!(rt_primes_1021bytes_4align_realloc_naive, 1021, 4);
rt_realloc!(rt_primes_1021bytes_4align_realloc, 1021, 4);
rt_realloc_excess_unused!(rt_primes_1021bytes_4align_realloc_excess_unused, 1021, 4);
rt_realloc_excess_used!(rt_primes_1021bytes_4align_realloc_excess_used, 1021, 4);

rt_calloc!(rt_primes_2039bytes_4align_calloc, 2039, 4);
rt_mallocx!(rt_primes_2039bytes_4align_mallocx, 2039, 4);
rt_mallocx_zeroed!(rt_primes_2039bytes_4align_mallocx_zeroed, 2039, 4);
rt_mallocx_nallocx!(rt_primes_2039bytes_4align_mallocx_nallocx, 2039, 4);
rt_alloc_layout_checked!(rt_primes_2039bytes_4align_alloc_layout_checked, 2039, 4);
rt_alloc_layout_unchecked!(rt_primes_2039bytes_4align_alloc_layout_unchecked, 2039, 4);
rt_alloc_excess_unused!(rt_primes_2039bytes_4align_alloc_excess_unused, 2039, 4);
rt_alloc_excess_used!(rt_primes_2039bytes_4align_alloc_excess_used, 2039, 4);
rt_realloc_naive!(rt_primes_2039bytes_4align_realloc_naive, 2039, 4);
rt_realloc!(rt_primes_2039bytes_4align_realloc, 2039, 4);
rt_realloc_excess_unused!(rt_primes_2039bytes_4align_realloc_excess_unused, 2039, 4);
rt_realloc_excess_used!(rt_primes_2039bytes_4align_realloc_excess_used, 2039, 4);

rt_calloc!(rt_primes_4093bytes_4align_calloc, 4093, 4);
rt_mallocx!(rt_primes_4093bytes_4align_mallocx, 4093, 4);
rt_mallocx_zeroed!(rt_primes_4093bytes_4align_mallocx_zeroed, 4093, 4);
rt_mallocx_nallocx!(rt_primes_4093bytes_4align_mallocx_nallocx, 4093, 4);
rt_alloc_layout_checked!(rt_primes_4093bytes_4align_alloc_layout_checked, 4093, 4);
rt_alloc_layout_unchecked!(rt_primes_4093bytes_4align_alloc_layout_unchecked, 4093, 4);
rt_alloc_excess_unused!(rt_primes_4093bytes_4align_alloc_excess_unused, 4093, 4);
rt_alloc_excess_used!(rt_primes_4093bytes_4align_alloc_excess_used, 4093, 4);
rt_realloc_naive!(rt_primes_4093bytes_4align_realloc_naive, 4093, 4);
rt_realloc!(rt_primes_4093bytes_4align_realloc, 4093, 4);
rt_realloc_excess_unused!(rt_primes_4093bytes_4align_realloc_excess_unused, 4093, 4);
rt_realloc_excess_used!(rt_primes_4093bytes_4align_realloc_excess_used, 4093, 4);

rt_calloc!(rt_primes_8191bytes_4align_calloc, 8191, 4);
rt_mallocx!(rt_primes_8191bytes_4align_mallocx, 8191, 4);
rt_mallocx_zeroed!(rt_primes_8191bytes_4align_mallocx_zeroed, 8191, 4);
rt_mallocx_nallocx!(rt_primes_8191bytes_4align_mallocx_nallocx, 8191, 4);
rt_alloc_layout_checked!(rt_primes_8191bytes_4align_alloc_layout_checked, 8191, 4);
rt_alloc_layout_unchecked!(rt_primes_8191bytes_4align_alloc_layout_unchecked, 8191, 4);
rt_alloc_excess_unused!(rt_primes_8191bytes_4align_alloc_excess_unused, 8191, 4);
rt_alloc_excess_used!(rt_primes_8191bytes_4align_alloc_excess_used, 8191, 4);
rt_realloc_naive!(rt_primes_8191bytes_4align_realloc_naive, 8191, 4);
rt_realloc!(rt_primes_8191bytes_4align_realloc, 8191, 4);
rt_realloc_excess_unused!(rt_primes_8191bytes_4align_realloc_excess_unused, 8191, 4);
rt_realloc_excess_used!(rt_primes_8191bytes_4align_realloc_excess_used, 8191, 4);

rt_calloc!(rt_primes_16381bytes_4align_calloc, 16381, 4);
rt_mallocx!(rt_primes_16381bytes_4align_mallocx, 16381, 4);
rt_mallocx_zeroed!(rt_primes_16381bytes_4align_mallocx_zeroed, 16381, 4);
rt_mallocx_nallocx!(rt_primes_16381bytes_4align_mallocx_nallocx, 16381, 4);
rt_alloc_layout_checked!(rt_primes_16381bytes_4align_alloc_layout_checked, 16381, 4);
rt_alloc_layout_unchecked!(rt_primes_16381bytes_4align_alloc_layout_unchecked, 16381, 4);
rt_alloc_excess_unused!(rt_primes_16381bytes_4align_alloc_excess_unused, 16381, 4);
rt_alloc_excess_used!(rt_primes_16381bytes_4align_alloc_excess_used, 16381, 4);
rt_realloc_naive!(rt_primes_16381bytes_4align_realloc_naive, 16381, 4);
rt_realloc!(rt_primes_16381bytes_4align_realloc, 16381, 4);
rt_realloc_excess_unused!(rt_primes_16381bytes_4align_realloc_excess_unused, 16381, 4);
rt_realloc_excess_used!(rt_primes_16381bytes_4align_realloc_excess_used, 16381, 4);

rt_calloc!(rt_primes_32749bytes_4align_calloc, 32749, 4);
rt_mallocx!(rt_primes_32749bytes_4align_mallocx, 32749, 4);
rt_mallocx_zeroed!(rt_primes_32749bytes_4align_mallocx_zeroed, 32749, 4);
rt_mallocx_nallocx!(rt_primes_32749bytes_4align_mallocx_nallocx, 32749, 4);
rt_alloc_layout_checked!(rt_primes_32749bytes_4align_alloc_layout_checked, 32749, 4);
rt_alloc_layout_unchecked!(rt_primes_32749bytes_4align_alloc_layout_unchecked, 32749, 4);
rt_alloc_excess_unused!(rt_primes_32749bytes_4align_alloc_excess_unused, 32749, 4);
rt_alloc_excess_used!(rt_primes_32749bytes_4align_alloc_excess_used, 32749, 4);
rt_realloc_naive!(rt_primes_32749bytes_4align_realloc_naive, 32749, 4);
rt_realloc!(rt_primes_32749bytes_4align_realloc, 32749, 4);
rt_realloc_excess_unused!(rt_primes_32749bytes_4align_realloc_excess_unused, 32749, 4);
rt_realloc_excess_used!(rt_primes_32749bytes_4align_realloc_excess_used, 32749, 4);

rt_calloc!(rt_primes_65537bytes_4align_calloc, 65537, 4);
rt_mallocx!(rt_primes_65537bytes_4align_mallocx, 65537, 4);
rt_mallocx_zeroed!(rt_primes_65537bytes_4align_mallocx_zeroed, 65537, 4);
rt_mallocx_nallocx!(rt_primes_65537bytes_4align_mallocx_nallocx, 65537, 4);
rt_alloc_layout_checked!(rt_primes_65537bytes_4align_alloc_layout_checked, 65537, 4);
rt_alloc_layout_unchecked!(rt_primes_65537bytes_4align_alloc_layout_unchecked, 65537, 4);
rt_alloc_excess_unused!(rt_primes_65537bytes_4align_alloc_excess_unused, 65537, 4);
rt_alloc_excess_used!(rt_primes_65537bytes_4align_alloc_excess_used, 65537, 4);
rt_realloc_naive!(rt_primes_65537bytes_4align_realloc_naive, 65537, 4);
rt_realloc!(rt_primes_65537bytes_4align_realloc, 65537, 4);
rt_realloc_excess_unused!(rt_primes_65537bytes_4align_realloc_excess_unused, 65537, 4);
rt_realloc_excess_used!(rt_primes_65537bytes_4align_realloc_excess_used, 65537, 4);

rt_calloc!(rt_primes_131071bytes_4align_calloc, 131071, 4);
rt_mallocx!(rt_primes_131071bytes_4align_mallocx, 131071, 4);
rt_mallocx_zeroed!(rt_primes_131071bytes_4align_mallocx_zeroed, 131071, 4);
rt_mallocx_nallocx!(rt_primes_131071bytes_4align_mallocx_nallocx, 131071, 4);
rt_alloc_layout_checked!(rt_primes_131071bytes_4align_alloc_layout_checked, 131071, 4);
rt_alloc_layout_unchecked!(
    rt_primes_131071bytes_4align_alloc_layout_unchecked,
    131071,
    4
);
rt_alloc_excess_unused!(rt_primes_131071bytes_4align_alloc_excess_unused, 131071, 4);
rt_alloc_excess_used!(rt_primes_131071bytes_4align_alloc_excess_used, 131071, 4);
rt_realloc_naive!(rt_primes_131071bytes_4align_realloc_naive, 131071, 4);
rt_realloc!(rt_primes_131071bytes_4align_realloc, 131071, 4);
rt_realloc_excess_unused!(
    rt_primes_131071bytes_4align_realloc_excess_unused,
    131071,
    4
);
rt_realloc_excess_used!(rt_primes_131071bytes_4align_realloc_excess_used, 131071, 4);

rt_calloc!(rt_primes_4194301bytes_4align_calloc, 4194301, 4);
rt_mallocx!(rt_primes_4194301bytes_4align_mallocx, 4194301, 4);
rt_mallocx_zeroed!(rt_primes_4194301bytes_4align_mallocx_zeroed, 4194301, 4);
rt_mallocx_nallocx!(rt_primes_4194301bytes_4align_mallocx_nallocx, 4194301, 4);
rt_alloc_layout_checked!(
    rt_primes_4194301bytes_4align_alloc_layout_checked,
    4194301,
    4
);
rt_alloc_layout_unchecked!(
    rt_primes_4194301bytes_4align_alloc_layout_unchecked,
    4194301,
    4
);
rt_alloc_excess_unused!(
    rt_primes_4194301bytes_4align_alloc_excess_unused,
    4194301,
    4
);
rt_alloc_excess_used!(rt_primes_4194301bytes_4align_alloc_excess_used, 4194301, 4);
rt_realloc_naive!(rt_primes_4194301bytes_4align_realloc_naive, 4194301, 4);
rt_realloc!(rt_primes_4194301bytes_4align_realloc, 4194301, 4);
rt_realloc_excess_unused!(
    rt_primes_4194301bytes_4align_realloc_excess_unused,
    4194301,
    4
);
rt_realloc_excess_used!(
    rt_primes_4194301bytes_4align_realloc_excess_used,
    4194301,
    4
);

// 8 bytes alignment

// Powers of two:
rt_calloc!(rt_pow2_1bytes_8align_calloc, 1, 8);
rt_mallocx!(rt_pow2_1bytes_8align_mallocx, 1, 8);
rt_mallocx_zeroed!(rt_pow2_1bytes_8align_mallocx_zeroed, 1, 8);
rt_mallocx_nallocx!(rt_pow2_1bytes_8align_mallocx_nallocx, 1, 8);
rt_alloc_layout_checked!(rt_pow2_1bytes_8align_alloc_layout_checked, 1, 8);
rt_alloc_layout_unchecked!(rt_pow2_1bytes_8align_alloc_layout_unchecked, 1, 8);
rt_alloc_excess_unused!(rt_pow2_1bytes_8align_alloc_excess_unused, 1, 8);
rt_alloc_excess_used!(rt_pow2_1bytes_8align_alloc_excess_used, 1, 8);
rt_realloc_naive!(rt_pow2_1bytes_8align_realloc_naive, 1, 8);
rt_realloc!(rt_pow2_1bytes_8align_realloc, 1, 8);
rt_realloc_excess_unused!(rt_pow2_1bytes_8align_realloc_excess_unused, 1, 8);
rt_realloc_excess_used!(rt_pow2_1bytes_8align_realloc_excess_used, 1, 8);

rt_calloc!(rt_pow2_2bytes_8align_calloc, 2, 8);
rt_mallocx!(rt_pow2_2bytes_8align_mallocx, 2, 8);
rt_mallocx_zeroed!(rt_pow2_2bytes_8align_mallocx_zeroed, 2, 8);
rt_mallocx_nallocx!(rt_pow2_2bytes_8align_mallocx_nallocx, 2, 8);
rt_alloc_layout_checked!(rt_pow2_2bytes_8align_alloc_layout_checked, 2, 8);
rt_alloc_layout_unchecked!(rt_pow2_2bytes_8align_alloc_layout_unchecked, 2, 8);
rt_alloc_excess_unused!(rt_pow2_2bytes_8align_alloc_excess_unused, 2, 8);
rt_alloc_excess_used!(rt_pow2_2bytes_8align_alloc_excess_used, 2, 8);
rt_realloc_naive!(rt_pow2_2bytes_8align_realloc_naive, 2, 8);
rt_realloc!(rt_pow2_2bytes_8align_realloc, 2, 8);
rt_realloc_excess_unused!(rt_pow2_2bytes_8align_realloc_excess_unused, 2, 8);
rt_realloc_excess_used!(rt_pow2_2bytes_8align_realloc_excess_used, 2, 8);

rt_calloc!(rt_pow2_4bytes_8align_calloc, 4, 8);
rt_mallocx!(rt_pow2_4bytes_8align_mallocx, 4, 8);
rt_mallocx_zeroed!(rt_pow2_4bytes_8align_mallocx_zeroed, 4, 8);
rt_mallocx_nallocx!(rt_pow2_4bytes_8align_mallocx_nallocx, 4, 8);
rt_alloc_layout_checked!(rt_pow2_4bytes_8align_alloc_layout_checked, 4, 8);
rt_alloc_layout_unchecked!(rt_pow2_4bytes_8align_alloc_layout_unchecked, 4, 8);
rt_alloc_excess_unused!(rt_pow2_4bytes_8align_alloc_excess_unused, 4, 8);
rt_alloc_excess_used!(rt_pow2_4bytes_8align_alloc_excess_used, 4, 8);
rt_realloc_naive!(rt_pow2_4bytes_8align_realloc_naive, 4, 8);
rt_realloc!(rt_pow2_4bytes_8align_realloc, 4, 8);
rt_realloc_excess_unused!(rt_pow2_4bytes_8align_realloc_excess_unused, 4, 8);
rt_realloc_excess_used!(rt_pow2_4bytes_8align_realloc_excess_used, 4, 8);

rt_calloc!(rt_pow2_8bytes_8align_calloc, 8, 8);
rt_mallocx!(rt_pow2_8bytes_8align_mallocx, 8, 8);
rt_mallocx_zeroed!(rt_pow2_8bytes_8align_mallocx_zeroed, 8, 8);
rt_mallocx_nallocx!(rt_pow2_8bytes_8align_mallocx_nallocx, 8, 8);
rt_alloc_layout_checked!(rt_pow2_8bytes_8align_alloc_layout_checked, 8, 8);
rt_alloc_layout_unchecked!(rt_pow2_8bytes_8align_alloc_layout_unchecked, 8, 8);
rt_alloc_excess_unused!(rt_pow2_8bytes_8align_alloc_excess_unused, 8, 8);
rt_alloc_excess_used!(rt_pow2_8bytes_8align_alloc_excess_used, 8, 8);
rt_realloc_naive!(rt_pow2_8bytes_8align_realloc_naive, 8, 8);
rt_realloc!(rt_pow2_8bytes_8align_realloc, 8, 8);
rt_realloc_excess_unused!(rt_pow2_8bytes_8align_realloc_excess_unused, 8, 8);
rt_realloc_excess_used!(rt_pow2_8bytes_8align_realloc_excess_used, 8, 8);

rt_calloc!(rt_pow2_16bytes_8align_calloc, 16, 8);
rt_mallocx!(rt_pow2_16bytes_8align_mallocx, 16, 8);
rt_mallocx_zeroed!(rt_pow2_16bytes_8align_mallocx_zeroed, 16, 8);
rt_mallocx_nallocx!(rt_pow2_16bytes_8align_mallocx_nallocx, 16, 8);
rt_alloc_layout_checked!(rt_pow2_16bytes_8align_alloc_layout_checked, 16, 8);
rt_alloc_layout_unchecked!(rt_pow2_16bytes_8align_alloc_layout_unchecked, 16, 8);
rt_alloc_excess_unused!(rt_pow2_16bytes_8align_alloc_excess_unused, 16, 8);
rt_alloc_excess_used!(rt_pow2_16bytes_8align_alloc_excess_used, 16, 8);
rt_realloc_naive!(rt_pow2_16bytes_8align_realloc_naive, 16, 8);
rt_realloc!(rt_pow2_16bytes_8align_realloc, 16, 8);
rt_realloc_excess_unused!(rt_pow2_16bytes_8align_realloc_excess_unused, 16, 8);
rt_realloc_excess_used!(rt_pow2_16bytes_8align_realloc_excess_used, 16, 8);

rt_calloc!(rt_pow2_32bytes_8align_calloc, 32, 8);
rt_mallocx!(rt_pow2_32bytes_8align_mallocx, 32, 8);
rt_mallocx_zeroed!(rt_pow2_32bytes_8align_mallocx_zeroed, 32, 8);
rt_mallocx_nallocx!(rt_pow2_32bytes_8align_mallocx_nallocx, 32, 8);
rt_alloc_layout_checked!(rt_pow2_32bytes_8align_alloc_layout_checked, 32, 8);
rt_alloc_layout_unchecked!(rt_pow2_32bytes_8align_alloc_layout_unchecked, 32, 8);
rt_alloc_excess_unused!(rt_pow2_32bytes_8align_alloc_excess_unused, 32, 8);
rt_alloc_excess_used!(rt_pow2_32bytes_8align_alloc_excess_used, 32, 8);
rt_realloc_naive!(rt_pow2_32bytes_8align_realloc_naive, 32, 8);
rt_realloc!(rt_pow2_32bytes_8align_realloc, 32, 8);
rt_realloc_excess_unused!(rt_pow2_32bytes_8align_realloc_excess_unused, 32, 8);
rt_realloc_excess_used!(rt_pow2_32bytes_8align_realloc_excess_used, 32, 8);

rt_calloc!(rt_pow2_64bytes_8align_calloc, 64, 8);
rt_mallocx!(rt_pow2_64bytes_8align_mallocx, 64, 8);
rt_mallocx_zeroed!(rt_pow2_64bytes_8align_mallocx_zeroed, 64, 8);
rt_mallocx_nallocx!(rt_pow2_64bytes_8align_mallocx_nallocx, 64, 8);
rt_alloc_layout_checked!(rt_pow2_64bytes_8align_alloc_layout_checked, 64, 8);
rt_alloc_layout_unchecked!(rt_pow2_64bytes_8align_alloc_layout_unchecked, 64, 8);
rt_alloc_excess_unused!(rt_pow2_64bytes_8align_alloc_excess_unused, 64, 8);
rt_alloc_excess_used!(rt_pow2_64bytes_8align_alloc_excess_used, 64, 8);
rt_realloc_naive!(rt_pow2_64bytes_8align_realloc_naive, 64, 8);
rt_realloc!(rt_pow2_64bytes_8align_realloc, 64, 8);
rt_realloc_excess_unused!(rt_pow2_64bytes_8align_realloc_excess_unused, 64, 8);
rt_realloc_excess_used!(rt_pow2_64bytes_8align_realloc_excess_used, 64, 8);

rt_calloc!(rt_pow2_128bytes_8align_calloc, 128, 8);
rt_mallocx!(rt_pow2_128bytes_8align_mallocx, 128, 8);
rt_mallocx_zeroed!(rt_pow2_128bytes_8align_mallocx_zeroed, 128, 8);
rt_mallocx_nallocx!(rt_pow2_128bytes_8align_mallocx_nallocx, 128, 8);
rt_alloc_layout_checked!(rt_pow2_128bytes_8align_alloc_layout_checked, 128, 8);
rt_alloc_layout_unchecked!(rt_pow2_128bytes_8align_alloc_layout_unchecked, 128, 8);
rt_alloc_excess_unused!(rt_pow2_128bytes_8align_alloc_excess_unused, 128, 8);
rt_alloc_excess_used!(rt_pow2_128bytes_8align_alloc_excess_used, 128, 8);
rt_realloc_naive!(rt_pow2_128bytes_8align_realloc_naive, 128, 8);
rt_realloc!(rt_pow2_128bytes_8align_realloc, 128, 8);
rt_realloc_excess_unused!(rt_pow2_128bytes_8align_realloc_excess_unused, 128, 8);
rt_realloc_excess_used!(rt_pow2_128bytes_8align_realloc_excess_used, 128, 8);

rt_calloc!(rt_pow2_256bytes_8align_calloc, 256, 8);
rt_mallocx!(rt_pow2_256bytes_8align_mallocx, 256, 8);
rt_mallocx_zeroed!(rt_pow2_256bytes_8align_mallocx_zeroed, 256, 8);
rt_mallocx_nallocx!(rt_pow2_256bytes_8align_mallocx_nallocx, 256, 8);
rt_alloc_layout_checked!(rt_pow2_256bytes_8align_alloc_layout_checked, 256, 8);
rt_alloc_layout_unchecked!(rt_pow2_256bytes_8align_alloc_layout_unchecked, 256, 8);
rt_alloc_excess_unused!(rt_pow2_256bytes_8align_alloc_excess_unused, 256, 8);
rt_alloc_excess_used!(rt_pow2_256bytes_8align_alloc_excess_used, 256, 8);
rt_realloc_naive!(rt_pow2_256bytes_8align_realloc_naive, 256, 8);
rt_realloc!(rt_pow2_256bytes_8align_realloc, 256, 8);
rt_realloc_excess_unused!(rt_pow2_256bytes_8align_realloc_excess_unused, 256, 8);
rt_realloc_excess_used!(rt_pow2_256bytes_8align_realloc_excess_used, 256, 8);

rt_calloc!(rt_pow2_512bytes_8align_calloc, 512, 8);
rt_mallocx!(rt_pow2_512bytes_8align_mallocx, 512, 8);
rt_mallocx_zeroed!(rt_pow2_512bytes_8align_mallocx_zeroed, 512, 8);
rt_mallocx_nallocx!(rt_pow2_512bytes_8align_mallocx_nallocx, 512, 8);
rt_alloc_layout_checked!(rt_pow2_512bytes_8align_alloc_layout_checked, 512, 8);
rt_alloc_layout_unchecked!(rt_pow2_512bytes_8align_alloc_layout_unchecked, 512, 8);
rt_alloc_excess_unused!(rt_pow2_512bytes_8align_alloc_excess_unused, 512, 8);
rt_alloc_excess_used!(rt_pow2_512bytes_8align_alloc_excess_used, 512, 8);
rt_realloc_naive!(rt_pow2_512bytes_8align_realloc_naive, 512, 8);
rt_realloc!(rt_pow2_512bytes_8align_realloc, 512, 8);
rt_realloc_excess_unused!(rt_pow2_512bytes_8align_realloc_excess_unused, 512, 8);
rt_realloc_excess_used!(rt_pow2_512bytes_8align_realloc_excess_used, 512, 8);

rt_calloc!(rt_pow2_1024bytes_8align_calloc, 1024, 8);
rt_mallocx!(rt_pow2_1024bytes_8align_mallocx, 1024, 8);
rt_mallocx_zeroed!(rt_pow2_1024bytes_8align_mallocx_zeroed, 1024, 8);
rt_mallocx_nallocx!(rt_pow2_1024bytes_8align_mallocx_nallocx, 1024, 8);
rt_alloc_layout_checked!(rt_pow2_1024bytes_8align_alloc_layout_checked, 1024, 8);
rt_alloc_layout_unchecked!(rt_pow2_1024bytes_8align_alloc_layout_unchecked, 1024, 8);
rt_alloc_excess_unused!(rt_pow2_1024bytes_8align_alloc_excess_unused, 1024, 8);
rt_alloc_excess_used!(rt_pow2_1024bytes_8align_alloc_excess_used, 1024, 8);
rt_realloc_naive!(rt_pow2_1024bytes_8align_realloc_naive, 1024, 8);
rt_realloc!(rt_pow2_1024bytes_8align_realloc, 1024, 8);
rt_realloc_excess_unused!(rt_pow2_1024bytes_8align_realloc_excess_unused, 1024, 8);
rt_realloc_excess_used!(rt_pow2_1024bytes_8align_realloc_excess_used, 1024, 8);

rt_calloc!(rt_pow2_2048bytes_8align_calloc, 2048, 8);
rt_mallocx!(rt_pow2_2048bytes_8align_mallocx, 2048, 8);
rt_mallocx_zeroed!(rt_pow2_2048bytes_8align_mallocx_zeroed, 2048, 8);
rt_mallocx_nallocx!(rt_pow2_2048bytes_8align_mallocx_nallocx, 2048, 8);
rt_alloc_layout_checked!(rt_pow2_2048bytes_8align_alloc_layout_checked, 2048, 8);
rt_alloc_layout_unchecked!(rt_pow2_2048bytes_8align_alloc_layout_unchecked, 2048, 8);
rt_alloc_excess_unused!(rt_pow2_2048bytes_8align_alloc_excess_unused, 2048, 8);
rt_alloc_excess_used!(rt_pow2_2048bytes_8align_alloc_excess_used, 2048, 8);
rt_realloc_naive!(rt_pow2_2048bytes_8align_realloc_naive, 2048, 8);
rt_realloc!(rt_pow2_2048bytes_8align_realloc, 2048, 8);
rt_realloc_excess_unused!(rt_pow2_2048bytes_8align_realloc_excess_unused, 2048, 8);
rt_realloc_excess_used!(rt_pow2_2048bytes_8align_realloc_excess_used, 2048, 8);

rt_calloc!(rt_pow2_4096bytes_8align_calloc, 4096, 8);
rt_mallocx!(rt_pow2_4096bytes_8align_mallocx, 4096, 8);
rt_mallocx_zeroed!(rt_pow2_4096bytes_8align_mallocx_zeroed, 4096, 8);
rt_mallocx_nallocx!(rt_pow2_4096bytes_8align_mallocx_nallocx, 4096, 8);
rt_alloc_layout_checked!(rt_pow2_4096bytes_8align_alloc_layout_checked, 4096, 8);
rt_alloc_layout_unchecked!(rt_pow2_4096bytes_8align_alloc_layout_unchecked, 4096, 8);
rt_alloc_excess_unused!(rt_pow2_4096bytes_8align_alloc_excess_unused, 4096, 8);
rt_alloc_excess_used!(rt_pow2_4096bytes_8align_alloc_excess_used, 4096, 8);
rt_realloc_naive!(rt_pow2_4096bytes_8align_realloc_naive, 4096, 8);
rt_realloc!(rt_pow2_4096bytes_8align_realloc, 4096, 8);
rt_realloc_excess_unused!(rt_pow2_4096bytes_8align_realloc_excess_unused, 4096, 8);
rt_realloc_excess_used!(rt_pow2_4096bytes_8align_realloc_excess_used, 4096, 8);

rt_calloc!(rt_pow2_8192bytes_8align_calloc, 8192, 8);
rt_mallocx!(rt_pow2_8192bytes_8align_mallocx, 8192, 8);
rt_mallocx_zeroed!(rt_pow2_8192bytes_8align_mallocx_zeroed, 8192, 8);
rt_mallocx_nallocx!(rt_pow2_8192bytes_8align_mallocx_nallocx, 8192, 8);
rt_alloc_layout_checked!(rt_pow2_8192bytes_8align_alloc_layout_checked, 8192, 8);
rt_alloc_layout_unchecked!(rt_pow2_8192bytes_8align_alloc_layout_unchecked, 8192, 8);
rt_alloc_excess_unused!(rt_pow2_8192bytes_8align_alloc_excess_unused, 8192, 8);
rt_alloc_excess_used!(rt_pow2_8192bytes_8align_alloc_excess_used, 8192, 8);
rt_realloc_naive!(rt_pow2_8192bytes_8align_realloc_naive, 8192, 8);
rt_realloc!(rt_pow2_8192bytes_8align_realloc, 8192, 8);
rt_realloc_excess_unused!(rt_pow2_8192bytes_8align_realloc_excess_unused, 8192, 8);
rt_realloc_excess_used!(rt_pow2_8192bytes_8align_realloc_excess_used, 8192, 8);

rt_calloc!(rt_pow2_16384bytes_8align_calloc, 16384, 8);
rt_mallocx!(rt_pow2_16384bytes_8align_mallocx, 16384, 8);
rt_mallocx_zeroed!(rt_pow2_16384bytes_8align_mallocx_zeroed, 16384, 8);
rt_mallocx_nallocx!(rt_pow2_16384bytes_8align_mallocx_nallocx, 16384, 8);
rt_alloc_layout_checked!(rt_pow2_16384bytes_8align_alloc_layout_checked, 16384, 8);
rt_alloc_layout_unchecked!(rt_pow2_16384bytes_8align_alloc_layout_unchecked, 16384, 8);
rt_alloc_excess_unused!(rt_pow2_16384bytes_8align_alloc_excess_unused, 16384, 8);
rt_alloc_excess_used!(rt_pow2_16384bytes_8align_alloc_excess_used, 16384, 8);
rt_realloc_naive!(rt_pow2_16384bytes_8align_realloc_naive, 16384, 8);
rt_realloc!(rt_pow2_16384bytes_8align_realloc, 16384, 8);
rt_realloc_excess_unused!(rt_pow2_16384bytes_8align_realloc_excess_unused, 16384, 8);
rt_realloc_excess_used!(rt_pow2_16384bytes_8align_realloc_excess_used, 16384, 8);

rt_calloc!(rt_pow2_32768bytes_8align_calloc, 32768, 8);
rt_mallocx!(rt_pow2_32768bytes_8align_mallocx, 32768, 8);
rt_mallocx_zeroed!(rt_pow2_32768bytes_8align_mallocx_zeroed, 32768, 8);
rt_mallocx_nallocx!(rt_pow2_32768bytes_8align_mallocx_nallocx, 32768, 8);
rt_alloc_layout_checked!(rt_pow2_32768bytes_8align_alloc_layout_checked, 32768, 8);
rt_alloc_layout_unchecked!(rt_pow2_32768bytes_8align_alloc_layout_unchecked, 32768, 8);
rt_alloc_excess_unused!(rt_pow2_32768bytes_8align_alloc_excess_unused, 32768, 8);
rt_alloc_excess_used!(rt_pow2_32768bytes_8align_alloc_excess_used, 32768, 8);
rt_realloc_naive!(rt_pow2_32768bytes_8align_realloc_naive, 32768, 8);
rt_realloc!(rt_pow2_32768bytes_8align_realloc, 32768, 8);
rt_realloc_excess_unused!(rt_pow2_32768bytes_8align_realloc_excess_unused, 32768, 8);
rt_realloc_excess_used!(rt_pow2_32768bytes_8align_realloc_excess_used, 32768, 8);

rt_calloc!(rt_pow2_65536bytes_8align_calloc, 65536, 8);
rt_mallocx!(rt_pow2_65536bytes_8align_mallocx, 65536, 8);
rt_mallocx_zeroed!(rt_pow2_65536bytes_8align_mallocx_zeroed, 65536, 8);
rt_mallocx_nallocx!(rt_pow2_65536bytes_8align_mallocx_nallocx, 65536, 8);
rt_alloc_layout_checked!(rt_pow2_65536bytes_8align_alloc_layout_checked, 65536, 8);
rt_alloc_layout_unchecked!(rt_pow2_65536bytes_8align_alloc_layout_unchecked, 65536, 8);
rt_alloc_excess_unused!(rt_pow2_65536bytes_8align_alloc_excess_unused, 65536, 8);
rt_alloc_excess_used!(rt_pow2_65536bytes_8align_alloc_excess_used, 65536, 8);
rt_realloc_naive!(rt_pow2_65536bytes_8align_realloc_naive, 65536, 8);
rt_realloc!(rt_pow2_65536bytes_8align_realloc, 65536, 8);
rt_realloc_excess_unused!(rt_pow2_65536bytes_8align_realloc_excess_unused, 65536, 8);
rt_realloc_excess_used!(rt_pow2_65536bytes_8align_realloc_excess_used, 65536, 8);

rt_calloc!(rt_pow2_131072bytes_8align_calloc, 131072, 8);
rt_mallocx!(rt_pow2_131072bytes_8align_mallocx, 131072, 8);
rt_mallocx_zeroed!(rt_pow2_131072bytes_8align_mallocx_zeroed, 131072, 8);
rt_mallocx_nallocx!(rt_pow2_131072bytes_8align_mallocx_nallocx, 131072, 8);
rt_alloc_layout_checked!(rt_pow2_131072bytes_8align_alloc_layout_checked, 131072, 8);
rt_alloc_layout_unchecked!(rt_pow2_131072bytes_8align_alloc_layout_unchecked, 131072, 8);
rt_alloc_excess_unused!(rt_pow2_131072bytes_8align_alloc_excess_unused, 131072, 8);
rt_alloc_excess_used!(rt_pow2_131072bytes_8align_alloc_excess_used, 131072, 8);
rt_realloc_naive!(rt_pow2_131072bytes_8align_realloc_naive, 131072, 8);
rt_realloc!(rt_pow2_131072bytes_8align_realloc, 131072, 8);
rt_realloc_excess_unused!(rt_pow2_131072bytes_8align_realloc_excess_unused, 131072, 8);
rt_realloc_excess_used!(rt_pow2_131072bytes_8align_realloc_excess_used, 131072, 8);

rt_calloc!(rt_pow2_4194304bytes_8align_calloc, 4194304, 8);
rt_mallocx!(rt_pow2_4194304bytes_8align_mallocx, 4194304, 8);
rt_mallocx_zeroed!(rt_pow2_4194304bytes_8align_mallocx_zeroed, 4194304, 8);
rt_mallocx_nallocx!(rt_pow2_4194304bytes_8align_mallocx_nallocx, 4194304, 8);
rt_alloc_layout_checked!(rt_pow2_4194304bytes_8align_alloc_layout_checked, 4194304, 8);
rt_alloc_layout_unchecked!(
    rt_pow2_4194304bytes_8align_alloc_layout_unchecked,
    4194304,
    8
);
rt_alloc_excess_unused!(rt_pow2_4194304bytes_8align_alloc_excess_unused, 4194304, 8);
rt_alloc_excess_used!(rt_pow2_4194304bytes_8align_alloc_excess_used, 4194304, 8);
rt_realloc_naive!(rt_pow2_4194304bytes_8align_realloc_naive, 4194304, 8);
rt_realloc!(rt_pow2_4194304bytes_8align_realloc, 4194304, 8);
rt_realloc_excess_unused!(
    rt_pow2_4194304bytes_8align_realloc_excess_unused,
    4194304,
    8
);
rt_realloc_excess_used!(rt_pow2_4194304bytes_8align_realloc_excess_used, 4194304, 8);

// Even
rt_calloc!(rt_even_10bytes_8align_calloc, 10, 8);
rt_mallocx!(rt_even_10bytes_8align_mallocx, 10, 8);
rt_mallocx_zeroed!(rt_even_10bytes_8align_mallocx_zeroed, 10, 8);
rt_mallocx_nallocx!(rt_even_10bytes_8align_mallocx_nallocx, 10, 8);
rt_alloc_layout_checked!(rt_even_10bytes_8align_alloc_layout_checked, 10, 8);
rt_alloc_layout_unchecked!(rt_even_10bytes_8align_alloc_layout_unchecked, 10, 8);
rt_alloc_excess_unused!(rt_even_10bytes_8align_alloc_excess_unused, 10, 8);
rt_alloc_excess_used!(rt_even_10bytes_8align_alloc_excess_used, 10, 8);
rt_realloc_naive!(rt_even_10bytes_8align_realloc_naive, 10, 8);
rt_realloc!(rt_even_10bytes_8align_realloc, 10, 8);
rt_realloc_excess_unused!(rt_even_10bytes_8align_realloc_excess_unused, 10, 8);
rt_realloc_excess_used!(rt_even_10bytes_8align_realloc_excess_used, 10, 8);

rt_calloc!(rt_even_100bytes_8align_calloc, 100, 8);
rt_mallocx!(rt_even_100bytes_8align_mallocx, 100, 8);
rt_mallocx_zeroed!(rt_even_100bytes_8align_mallocx_zeroed, 100, 8);
rt_mallocx_nallocx!(rt_even_100bytes_8align_mallocx_nallocx, 100, 8);
rt_alloc_layout_checked!(rt_even_100bytes_8align_alloc_layout_checked, 100, 8);
rt_alloc_layout_unchecked!(rt_even_100bytes_8align_alloc_layout_unchecked, 100, 8);
rt_alloc_excess_unused!(rt_even_100bytes_8align_alloc_excess_unused, 100, 8);
rt_alloc_excess_used!(rt_even_100bytes_8align_alloc_excess_used, 100, 8);
rt_realloc_naive!(rt_even_100bytes_8align_realloc_naive, 100, 8);
rt_realloc!(rt_even_100bytes_8align_realloc, 100, 8);
rt_realloc_excess_unused!(rt_even_100bytes_8align_realloc_excess_unused, 100, 8);
rt_realloc_excess_used!(rt_even_100bytes_8align_realloc_excess_used, 100, 8);

rt_calloc!(rt_even_1000bytes_8align_calloc, 1000, 8);
rt_mallocx!(rt_even_1000bytes_8align_mallocx, 1000, 8);
rt_mallocx_zeroed!(rt_even_1000bytes_8align_mallocx_zeroed, 1000, 8);
rt_mallocx_nallocx!(rt_even_1000bytes_8align_mallocx_nallocx, 1000, 8);
rt_alloc_layout_checked!(rt_even_1000bytes_8align_alloc_layout_checked, 1000, 8);
rt_alloc_layout_unchecked!(rt_even_1000bytes_8align_alloc_layout_unchecked, 1000, 8);
rt_alloc_excess_unused!(rt_even_1000bytes_8align_alloc_excess_unused, 1000, 8);
rt_alloc_excess_used!(rt_even_1000bytes_8align_alloc_excess_used, 1000, 8);
rt_realloc_naive!(rt_even_1000bytes_8align_realloc_naive, 1000, 8);
rt_realloc!(rt_even_1000bytes_8align_realloc, 1000, 8);
rt_realloc_excess_unused!(rt_even_1000bytes_8align_realloc_excess_unused, 1000, 8);
rt_realloc_excess_used!(rt_even_1000bytes_8align_realloc_excess_used, 1000, 8);

rt_calloc!(rt_even_10000bytes_8align_calloc, 10000, 8);
rt_mallocx!(rt_even_10000bytes_8align_mallocx, 10000, 8);
rt_mallocx_zeroed!(rt_even_10000bytes_8align_mallocx_zeroed, 10000, 8);
rt_mallocx_nallocx!(rt_even_10000bytes_8align_mallocx_nallocx, 10000, 8);
rt_alloc_layout_checked!(rt_even_10000bytes_8align_alloc_layout_checked, 10000, 8);
rt_alloc_layout_unchecked!(rt_even_10000bytes_8align_alloc_layout_unchecked, 10000, 8);
rt_alloc_excess_unused!(rt_even_10000bytes_8align_alloc_excess_unused, 10000, 8);
rt_alloc_excess_used!(rt_even_10000bytes_8align_alloc_excess_used, 10000, 8);
rt_realloc_naive!(rt_even_10000bytes_8align_realloc_naive, 10000, 8);
rt_realloc!(rt_even_10000bytes_8align_realloc, 10000, 8);
rt_realloc_excess_unused!(rt_even_10000bytes_8align_realloc_excess_unused, 10000, 8);
rt_realloc_excess_used!(rt_even_10000bytes_8align_realloc_excess_used, 10000, 8);

rt_calloc!(rt_even_100000bytes_8align_calloc, 100000, 8);
rt_mallocx!(rt_even_100000bytes_8align_mallocx, 100000, 8);
rt_mallocx_zeroed!(rt_even_100000bytes_8align_mallocx_zeroed, 100000, 8);
rt_mallocx_nallocx!(rt_even_100000bytes_8align_mallocx_nallocx, 100000, 8);
rt_alloc_layout_checked!(rt_even_100000bytes_8align_alloc_layout_checked, 100000, 8);
rt_alloc_layout_unchecked!(rt_even_100000bytes_8align_alloc_layout_unchecked, 100000, 8);
rt_alloc_excess_unused!(rt_even_100000bytes_8align_alloc_excess_unused, 100000, 8);
rt_alloc_excess_used!(rt_even_100000bytes_8align_alloc_excess_used, 100000, 8);
rt_realloc_naive!(rt_even_100000bytes_8align_realloc_naive, 100000, 8);
rt_realloc!(rt_even_100000bytes_8align_realloc, 100000, 8);
rt_realloc_excess_unused!(rt_even_100000bytes_8align_realloc_excess_unused, 100000, 8);
rt_realloc_excess_used!(rt_even_100000bytes_8align_realloc_excess_used, 100000, 8);

rt_calloc!(rt_even_1000000bytes_8align_calloc, 1000000, 8);
rt_mallocx!(rt_even_1000000bytes_8align_mallocx, 1000000, 8);
rt_mallocx_zeroed!(rt_even_1000000bytes_8align_mallocx_zeroed, 1000000, 8);
rt_mallocx_nallocx!(rt_even_1000000bytes_8align_mallocx_nallocx, 1000000, 8);
rt_alloc_layout_checked!(rt_even_1000000bytes_8align_alloc_layout_checked, 1000000, 8);
rt_alloc_layout_unchecked!(
    rt_even_1000000bytes_8align_alloc_layout_unchecked,
    1000000,
    8
);
rt_alloc_excess_unused!(rt_even_1000000bytes_8align_alloc_excess_unused, 1000000, 8);
rt_alloc_excess_used!(rt_even_1000000bytes_8align_alloc_excess_used, 1000000, 8);
rt_realloc_naive!(rt_even_1000000bytes_8align_realloc_naive, 1000000, 8);
rt_realloc!(rt_even_1000000bytes_8align_realloc, 1000000, 8);
rt_realloc_excess_unused!(
    rt_even_1000000bytes_8align_realloc_excess_unused,
    1000000,
    8
);
rt_realloc_excess_used!(rt_even_1000000bytes_8align_realloc_excess_used, 1000000, 8);

// Odd:
rt_calloc!(rt_odd_10bytes_8align_calloc, 10 - 1, 8);
rt_mallocx!(rt_odd_10bytes_8align_mallocx, 10 - 1, 8);
rt_mallocx_zeroed!(rt_odd_10bytes_8align_mallocx_zeroed, 10 - 1, 8);
rt_mallocx_nallocx!(rt_odd_10bytes_8align_mallocx_nallocx, 10 - 1, 8);
rt_alloc_layout_checked!(rt_odd_10bytes_8align_alloc_layout_checked, 10 - 1, 8);
rt_alloc_layout_unchecked!(rt_odd_10bytes_8align_alloc_layout_unchecked, 10 - 1, 8);
rt_alloc_excess_unused!(rt_odd_10bytes_8align_alloc_excess_unused, 10 - 1, 8);
rt_alloc_excess_used!(rt_odd_10bytes_8align_alloc_excess_used, 10 - 1, 8);
rt_realloc_naive!(rt_odd_10bytes_8align_realloc_naive, 10 - 1, 8);
rt_realloc!(rt_odd_10bytes_8align_realloc, 10 - 1, 8);
rt_realloc_excess_unused!(rt_odd_10bytes_8align_realloc_excess_unused, 10 - 1, 8);
rt_realloc_excess_used!(rt_odd_10bytes_8align_realloc_excess_used, 10 - 1, 8);

rt_calloc!(rt_odd_100bytes_8align_calloc, 100 - 1, 8);
rt_mallocx!(rt_odd_100bytes_8align_mallocx, 100 - 1, 8);
rt_mallocx_zeroed!(rt_odd_100bytes_8align_mallocx_zeroed, 100 - 1, 8);
rt_mallocx_nallocx!(rt_odd_100bytes_8align_mallocx_nallocx, 100 - 1, 8);
rt_alloc_layout_checked!(rt_odd_100bytes_8align_alloc_layout_checked, 100 - 1, 8);
rt_alloc_layout_unchecked!(rt_odd_100bytes_8align_alloc_layout_unchecked, 100 - 1, 8);
rt_alloc_excess_unused!(rt_odd_100bytes_8align_alloc_excess_unused, 100 - 1, 8);
rt_alloc_excess_used!(rt_odd_100bytes_8align_alloc_excess_used, 100 - 1, 8);
rt_realloc_naive!(rt_odd_100bytes_8align_realloc_naive, 100 - 1, 8);
rt_realloc!(rt_odd_100bytes_8align_realloc, 100 - 1, 8);
rt_realloc_excess_unused!(rt_odd_100bytes_8align_realloc_excess_unused, 100 - 1, 8);
rt_realloc_excess_used!(rt_odd_100bytes_8align_realloc_excess_used, 100 - 1, 8);

rt_calloc!(rt_odd_1000bytes_8align_calloc, 1000 - 1, 8);
rt_mallocx!(rt_odd_1000bytes_8align_mallocx, 1000 - 1, 8);
rt_mallocx_zeroed!(rt_odd_1000bytes_8align_mallocx_zeroed, 1000 - 1, 8);
rt_mallocx_nallocx!(rt_odd_1000bytes_8align_mallocx_nallocx, 1000 - 1, 8);
rt_alloc_layout_checked!(rt_odd_1000bytes_8align_alloc_layout_checked, 1000 - 1, 8);
rt_alloc_layout_unchecked!(rt_odd_1000bytes_8align_alloc_layout_unchecked, 1000 - 1, 8);
rt_alloc_excess_unused!(rt_odd_1000bytes_8align_alloc_excess_unused, 1000 - 1, 8);
rt_alloc_excess_used!(rt_odd_1000bytes_8align_alloc_excess_used, 1000 - 1, 8);
rt_realloc_naive!(rt_odd_1000bytes_8align_realloc_naive, 1000 - 1, 8);
rt_realloc!(rt_odd_1000bytes_8align_realloc, 1000 - 1, 8);
rt_realloc_excess_unused!(rt_odd_1000bytes_8align_realloc_excess_unused, 1000 - 1, 8);
rt_realloc_excess_used!(rt_odd_1000bytes_8align_realloc_excess_used, 1000 - 1, 8);

rt_calloc!(rt_odd_10000bytes_8align_calloc, 10000 - 1, 8);
rt_mallocx!(rt_odd_10000bytes_8align_mallocx, 10000 - 1, 8);
rt_mallocx_zeroed!(rt_odd_10000bytes_8align_mallocx_zeroed, 10000 - 1, 8);
rt_mallocx_nallocx!(rt_odd_10000bytes_8align_mallocx_nallocx, 10000 - 1, 8);
rt_alloc_layout_checked!(rt_odd_10000bytes_8align_alloc_layout_checked, 10000 - 1, 8);
rt_alloc_layout_unchecked!(
    rt_odd_10000bytes_8align_alloc_layout_unchecked,
    10000 - 1,
    8
);
rt_alloc_excess_unused!(rt_odd_10000bytes_8align_alloc_excess_unused, 10000 - 1, 8);
rt_alloc_excess_used!(rt_odd_10000bytes_8align_alloc_excess_used, 10000 - 1, 8);
rt_realloc_naive!(rt_odd_10000bytes_8align_realloc_naive, 10000 - 1, 8);
rt_realloc!(rt_odd_10000bytes_8align_realloc, 10000 - 1, 8);
rt_realloc_excess_unused!(rt_odd_10000bytes_8align_realloc_excess_unused, 10000 - 1, 8);
rt_realloc_excess_used!(rt_odd_10000bytes_8align_realloc_excess_used, 10000 - 1, 8);

rt_calloc!(rt_odd_100000bytes_8align_calloc, 100000 - 1, 8);
rt_mallocx!(rt_odd_100000bytes_8align_mallocx, 100000 - 1, 8);
rt_mallocx_zeroed!(rt_odd_100000bytes_8align_mallocx_zeroed, 100000 - 1, 8);
rt_mallocx_nallocx!(rt_odd_100000bytes_8align_mallocx_nallocx, 100000 - 1, 8);
rt_alloc_layout_checked!(
    rt_odd_100000bytes_8align_alloc_layout_checked,
    100000 - 1,
    8
);
rt_alloc_layout_unchecked!(
    rt_odd_100000bytes_8align_alloc_layout_unchecked,
    100000 - 1,
    8
);
rt_alloc_excess_unused!(rt_odd_100000bytes_8align_alloc_excess_unused, 100000 - 1, 8);
rt_alloc_excess_used!(rt_odd_100000bytes_8align_alloc_excess_used, 100000 - 1, 8);
rt_realloc_naive!(rt_odd_100000bytes_8align_realloc_naive, 100000 - 1, 8);
rt_realloc!(rt_odd_100000bytes_8align_realloc, 100000 - 1, 8);
rt_realloc_excess_unused!(
    rt_odd_100000bytes_8align_realloc_excess_unused,
    100000 - 1,
    8
);
rt_realloc_excess_used!(rt_odd_100000bytes_8align_realloc_excess_used, 100000 - 1, 8);

rt_calloc!(rt_odd_1000000bytes_8align_calloc, 1000000 - 1, 8);
rt_mallocx!(rt_odd_1000000bytes_8align_mallocx, 1000000 - 1, 8);
rt_mallocx_zeroed!(rt_odd_1000000bytes_8align_mallocx_zeroed, 1000000 - 1, 8);
rt_mallocx_nallocx!(rt_odd_1000000bytes_8align_mallocx_nallocx, 1000000 - 1, 8);
rt_alloc_layout_checked!(
    rt_odd_1000000bytes_8align_alloc_layout_checked,
    1000000 - 1,
    8
);
rt_alloc_layout_unchecked!(
    rt_odd_1000000bytes_8align_alloc_layout_unchecked,
    1000000 - 1,
    8
);
rt_alloc_excess_unused!(
    rt_odd_1000000bytes_8align_alloc_excess_unused,
    1000000 - 1,
    8
);
rt_alloc_excess_used!(rt_odd_1000000bytes_8align_alloc_excess_used, 1000000 - 1, 8);
rt_realloc_naive!(rt_odd_1000000bytes_8align_realloc_naive, 1000000 - 1, 8);
rt_realloc!(rt_odd_1000000bytes_8align_realloc, 1000000 - 1, 8);
rt_realloc_excess_unused!(
    rt_odd_1000000bytes_8align_realloc_excess_unused,
    1000000 - 1,
    8
);
rt_realloc_excess_used!(
    rt_odd_1000000bytes_8align_realloc_excess_used,
    1000000 - 1,
    8
);

// primes
rt_calloc!(rt_primes_3bytes_8align_calloc, 3, 8);
rt_mallocx!(rt_primes_3bytes_8align_mallocx, 3, 8);
rt_mallocx_zeroed!(rt_primes_3bytes_8align_mallocx_zeroed, 3, 8);
rt_mallocx_nallocx!(rt_primes_3bytes_8align_mallocx_nallocx, 3, 8);
rt_alloc_layout_checked!(rt_primes_3bytes_8align_alloc_layout_checked, 3, 8);
rt_alloc_layout_unchecked!(rt_primes_3bytes_8align_alloc_layout_unchecked, 3, 8);
rt_alloc_excess_unused!(rt_primes_3bytes_8align_alloc_excess_unused, 3, 8);
rt_alloc_excess_used!(rt_primes_3bytes_8align_alloc_excess_used, 3, 8);
rt_realloc_naive!(rt_primes_3bytes_8align_realloc_naive, 3, 8);
rt_realloc!(rt_primes_3bytes_8align_realloc, 3, 8);
rt_realloc_excess_unused!(rt_primes_3bytes_8align_realloc_excess_unused, 3, 8);
rt_realloc_excess_used!(rt_primes_3bytes_8align_realloc_excess_used, 3, 8);

rt_calloc!(rt_primes_7bytes_8align_calloc, 7, 8);
rt_mallocx!(rt_primes_7bytes_8align_mallocx, 7, 8);
rt_mallocx_zeroed!(rt_primes_7bytes_8align_mallocx_zeroed, 7, 8);
rt_mallocx_nallocx!(rt_primes_7bytes_8align_mallocx_nallocx, 7, 8);
rt_alloc_layout_checked!(rt_primes_7bytes_8align_alloc_layout_checked, 7, 8);
rt_alloc_layout_unchecked!(rt_primes_7bytes_8align_alloc_layout_unchecked, 7, 8);
rt_alloc_excess_unused!(rt_primes_7bytes_8align_alloc_excess_unused, 7, 8);
rt_alloc_excess_used!(rt_primes_7bytes_8align_alloc_excess_used, 7, 8);
rt_realloc_naive!(rt_primes_7bytes_8align_realloc_naive, 7, 8);
rt_realloc!(rt_primes_7bytes_8align_realloc, 7, 8);
rt_realloc_excess_unused!(rt_primes_7bytes_8align_realloc_excess_unused, 7, 8);
rt_realloc_excess_used!(rt_primes_7bytes_8align_realloc_excess_used, 7, 8);

rt_calloc!(rt_primes_13bytes_8align_calloc, 13, 8);
rt_mallocx!(rt_primes_13bytes_8align_mallocx, 13, 8);
rt_mallocx_zeroed!(rt_primes_13bytes_8align_mallocx_zeroed, 13, 8);
rt_mallocx_nallocx!(rt_primes_13bytes_8align_mallocx_nallocx, 13, 8);
rt_alloc_layout_checked!(rt_primes_13bytes_8align_alloc_layout_checked, 13, 8);
rt_alloc_layout_unchecked!(rt_primes_13bytes_8align_alloc_layout_unchecked, 13, 8);
rt_alloc_excess_unused!(rt_primes_13bytes_8align_alloc_excess_unused, 13, 8);
rt_alloc_excess_used!(rt_primes_13bytes_8align_alloc_excess_used, 13, 8);
rt_realloc_naive!(rt_primes_13bytes_8align_realloc_naive, 13, 8);
rt_realloc!(rt_primes_13bytes_8align_realloc, 13, 8);
rt_realloc_excess_unused!(rt_primes_13bytes_8align_realloc_excess_unused, 13, 8);
rt_realloc_excess_used!(rt_primes_13bytes_8align_realloc_excess_used, 13, 8);

rt_calloc!(rt_primes_17bytes_8align_calloc, 17, 8);
rt_mallocx!(rt_primes_17bytes_8align_mallocx, 17, 8);
rt_mallocx_zeroed!(rt_primes_17bytes_8align_mallocx_zeroed, 17, 8);
rt_mallocx_nallocx!(rt_primes_17bytes_8align_mallocx_nallocx, 17, 8);
rt_alloc_layout_checked!(rt_primes_17bytes_8align_alloc_layout_checked, 17, 8);
rt_alloc_layout_unchecked!(rt_primes_17bytes_8align_alloc_layout_unchecked, 17, 8);
rt_alloc_excess_unused!(rt_primes_17bytes_8align_alloc_excess_unused, 17, 8);
rt_alloc_excess_used!(rt_primes_17bytes_8align_alloc_excess_used, 17, 8);
rt_realloc_naive!(rt_primes_17bytes_8align_realloc_naive, 17, 8);
rt_realloc!(rt_primes_17bytes_8align_realloc, 17, 8);
rt_realloc_excess_unused!(rt_primes_17bytes_8align_realloc_excess_unused, 17, 8);
rt_realloc_excess_used!(rt_primes_17bytes_8align_realloc_excess_used, 17, 8);

rt_calloc!(rt_primes_31bytes_8align_calloc, 31, 8);
rt_mallocx!(rt_primes_31bytes_8align_mallocx, 31, 8);
rt_mallocx_zeroed!(rt_primes_31bytes_8align_mallocx_zeroed, 31, 8);
rt_mallocx_nallocx!(rt_primes_31bytes_8align_mallocx_nallocx, 31, 8);
rt_alloc_layout_checked!(rt_primes_31bytes_8align_alloc_layout_checked, 31, 8);
rt_alloc_layout_unchecked!(rt_primes_31bytes_8align_alloc_layout_unchecked, 31, 8);
rt_alloc_excess_unused!(rt_primes_31bytes_8align_alloc_excess_unused, 31, 8);
rt_alloc_excess_used!(rt_primes_31bytes_8align_alloc_excess_used, 31, 8);
rt_realloc_naive!(rt_primes_31bytes_8align_realloc_naive, 31, 8);
rt_realloc!(rt_primes_31bytes_8align_realloc, 31, 8);
rt_realloc_excess_unused!(rt_primes_31bytes_8align_realloc_excess_unused, 31, 8);
rt_realloc_excess_used!(rt_primes_31bytes_8align_realloc_excess_used, 31, 8);

rt_calloc!(rt_primes_61bytes_8align_calloc, 61, 8);
rt_mallocx!(rt_primes_61bytes_8align_mallocx, 61, 8);
rt_mallocx_zeroed!(rt_primes_61bytes_8align_mallocx_zeroed, 61, 8);
rt_mallocx_nallocx!(rt_primes_61bytes_8align_mallocx_nallocx, 61, 8);
rt_alloc_layout_checked!(rt_primes_61bytes_8align_alloc_layout_checked, 61, 8);
rt_alloc_layout_unchecked!(rt_primes_61bytes_8align_alloc_layout_unchecked, 61, 8);
rt_alloc_excess_unused!(rt_primes_61bytes_8align_alloc_excess_unused, 61, 8);
rt_alloc_excess_used!(rt_primes_61bytes_8align_alloc_excess_used, 61, 8);
rt_realloc_naive!(rt_primes_61bytes_8align_realloc_naive, 61, 8);
rt_realloc!(rt_primes_61bytes_8align_realloc, 61, 8);
rt_realloc_excess_unused!(rt_primes_61bytes_8align_realloc_excess_unused, 61, 8);
rt_realloc_excess_used!(rt_primes_61bytes_8align_realloc_excess_used, 61, 8);

rt_calloc!(rt_primes_96bytes_8align_calloc, 96, 8);
rt_mallocx!(rt_primes_96bytes_8align_mallocx, 96, 8);
rt_mallocx_zeroed!(rt_primes_96bytes_8align_mallocx_zeroed, 96, 8);
rt_mallocx_nallocx!(rt_primes_96bytes_8align_mallocx_nallocx, 96, 8);
rt_alloc_layout_checked!(rt_primes_96bytes_8align_alloc_layout_checked, 96, 8);
rt_alloc_layout_unchecked!(rt_primes_96bytes_8align_alloc_layout_unchecked, 96, 8);
rt_alloc_excess_unused!(rt_primes_96bytes_8align_alloc_excess_unused, 96, 8);
rt_alloc_excess_used!(rt_primes_96bytes_8align_alloc_excess_used, 96, 8);
rt_realloc_naive!(rt_primes_96bytes_8align_realloc_naive, 96, 8);
rt_realloc!(rt_primes_96bytes_8align_realloc, 96, 8);
rt_realloc_excess_unused!(rt_primes_96bytes_8align_realloc_excess_unused, 96, 8);
rt_realloc_excess_used!(rt_primes_96bytes_8align_realloc_excess_used, 96, 8);

rt_calloc!(rt_primes_127bytes_8align_calloc, 127, 8);
rt_mallocx!(rt_primes_127bytes_8align_mallocx, 127, 8);
rt_mallocx_zeroed!(rt_primes_127bytes_8align_mallocx_zeroed, 127, 8);
rt_mallocx_nallocx!(rt_primes_127bytes_8align_mallocx_nallocx, 127, 8);
rt_alloc_layout_checked!(rt_primes_127bytes_8align_alloc_layout_checked, 127, 8);
rt_alloc_layout_unchecked!(rt_primes_127bytes_8align_alloc_layout_unchecked, 127, 8);
rt_alloc_excess_unused!(rt_primes_127bytes_8align_alloc_excess_unused, 127, 8);
rt_alloc_excess_used!(rt_primes_127bytes_8align_alloc_excess_used, 127, 8);
rt_realloc_naive!(rt_primes_127bytes_8align_realloc_naive, 127, 8);
rt_realloc!(rt_primes_127bytes_8align_realloc, 127, 8);
rt_realloc_excess_unused!(rt_primes_127bytes_8align_realloc_excess_unused, 127, 8);
rt_realloc_excess_used!(rt_primes_127bytes_8align_realloc_excess_used, 127, 8);

rt_calloc!(rt_primes_257bytes_8align_calloc, 257, 8);
rt_mallocx!(rt_primes_257bytes_8align_mallocx, 257, 8);
rt_mallocx_zeroed!(rt_primes_257bytes_8align_mallocx_zeroed, 257, 8);
rt_mallocx_nallocx!(rt_primes_257bytes_8align_mallocx_nallocx, 257, 8);
rt_alloc_layout_checked!(rt_primes_257bytes_8align_alloc_layout_checked, 257, 8);
rt_alloc_layout_unchecked!(rt_primes_257bytes_8align_alloc_layout_unchecked, 257, 8);
rt_alloc_excess_unused!(rt_primes_257bytes_8align_alloc_excess_unused, 257, 8);
rt_alloc_excess_used!(rt_primes_257bytes_8align_alloc_excess_used, 257, 8);
rt_realloc_naive!(rt_primes_257bytes_8align_realloc_naive, 257, 8);
rt_realloc!(rt_primes_257bytes_8align_realloc, 257, 8);
rt_realloc_excess_unused!(rt_primes_257bytes_8align_realloc_excess_unused, 257, 8);
rt_realloc_excess_used!(rt_primes_257bytes_8align_realloc_excess_used, 257, 8);

rt_calloc!(rt_primes_509bytes_8align_calloc, 509, 8);
rt_mallocx!(rt_primes_509bytes_8align_mallocx, 509, 8);
rt_mallocx_zeroed!(rt_primes_509bytes_8align_mallocx_zeroed, 509, 8);
rt_mallocx_nallocx!(rt_primes_509bytes_8align_mallocx_nallocx, 509, 8);
rt_alloc_layout_checked!(rt_primes_509bytes_8align_alloc_layout_checked, 509, 8);
rt_alloc_layout_unchecked!(rt_primes_509bytes_8align_alloc_layout_unchecked, 509, 8);
rt_alloc_excess_unused!(rt_primes_509bytes_8align_alloc_excess_unused, 509, 8);
rt_alloc_excess_used!(rt_primes_509bytes_8align_alloc_excess_used, 509, 8);
rt_realloc_naive!(rt_primes_509bytes_8align_realloc_naive, 509, 8);
rt_realloc!(rt_primes_509bytes_8align_realloc, 509, 8);
rt_realloc_excess_unused!(rt_primes_509bytes_8align_realloc_excess_unused, 509, 8);
rt_realloc_excess_used!(rt_primes_509bytes_8align_realloc_excess_used, 509, 8);

rt_calloc!(rt_primes_1021bytes_8align_calloc, 1021, 8);
rt_mallocx!(rt_primes_1021bytes_8align_mallocx, 1021, 8);
rt_mallocx_zeroed!(rt_primes_1021bytes_8align_mallocx_zeroed, 1021, 8);
rt_mallocx_nallocx!(rt_primes_1021bytes_8align_mallocx_nallocx, 1021, 8);
rt_alloc_layout_checked!(rt_primes_1021bytes_8align_alloc_layout_checked, 1021, 8);
rt_alloc_layout_unchecked!(rt_primes_1021bytes_8align_alloc_layout_unchecked, 1021, 8);
rt_alloc_excess_unused!(rt_primes_1021bytes_8align_alloc_excess_unused, 1021, 8);
rt_alloc_excess_used!(rt_primes_1021bytes_8align_alloc_excess_used, 1021, 8);
rt_realloc_naive!(rt_primes_1021bytes_8align_realloc_naive, 1021, 8);
rt_realloc!(rt_primes_1021bytes_8align_realloc, 1021, 8);
rt_realloc_excess_unused!(rt_primes_1021bytes_8align_realloc_excess_unused, 1021, 8);
rt_realloc_excess_used!(rt_primes_1021bytes_8align_realloc_excess_used, 1021, 8);

rt_calloc!(rt_primes_2039bytes_8align_calloc, 2039, 8);
rt_mallocx!(rt_primes_2039bytes_8align_mallocx, 2039, 8);
rt_mallocx_zeroed!(rt_primes_2039bytes_8align_mallocx_zeroed, 2039, 8);
rt_mallocx_nallocx!(rt_primes_2039bytes_8align_mallocx_nallocx, 2039, 8);
rt_alloc_layout_checked!(rt_primes_2039bytes_8align_alloc_layout_checked, 2039, 8);
rt_alloc_layout_unchecked!(rt_primes_2039bytes_8align_alloc_layout_unchecked, 2039, 8);
rt_alloc_excess_unused!(rt_primes_2039bytes_8align_alloc_excess_unused, 2039, 8);
rt_alloc_excess_used!(rt_primes_2039bytes_8align_alloc_excess_used, 2039, 8);
rt_realloc_naive!(rt_primes_2039bytes_8align_realloc_naive, 2039, 8);
rt_realloc!(rt_primes_2039bytes_8align_realloc, 2039, 8);
rt_realloc_excess_unused!(rt_primes_2039bytes_8align_realloc_excess_unused, 2039, 8);
rt_realloc_excess_used!(rt_primes_2039bytes_8align_realloc_excess_used, 2039, 8);

rt_calloc!(rt_primes_4093bytes_8align_calloc, 4093, 8);
rt_mallocx!(rt_primes_4093bytes_8align_mallocx, 4093, 8);
rt_mallocx_zeroed!(rt_primes_4093bytes_8align_mallocx_zeroed, 4093, 8);
rt_mallocx_nallocx!(rt_primes_4093bytes_8align_mallocx_nallocx, 4093, 8);
rt_alloc_layout_checked!(rt_primes_4093bytes_8align_alloc_layout_checked, 4093, 8);
rt_alloc_layout_unchecked!(rt_primes_4093bytes_8align_alloc_layout_unchecked, 4093, 8);
rt_alloc_excess_unused!(rt_primes_4093bytes_8align_alloc_excess_unused, 4093, 8);
rt_alloc_excess_used!(rt_primes_4093bytes_8align_alloc_excess_used, 4093, 8);
rt_realloc_naive!(rt_primes_4093bytes_8align_realloc_naive, 4093, 8);
rt_realloc!(rt_primes_4093bytes_8align_realloc, 4093, 8);
rt_realloc_excess_unused!(rt_primes_4093bytes_8align_realloc_excess_unused, 4093, 8);
rt_realloc_excess_used!(rt_primes_4093bytes_8align_realloc_excess_used, 4093, 8);

rt_calloc!(rt_primes_8191bytes_8align_calloc, 8191, 8);
rt_mallocx!(rt_primes_8191bytes_8align_mallocx, 8191, 8);
rt_mallocx_zeroed!(rt_primes_8191bytes_8align_mallocx_zeroed, 8191, 8);
rt_mallocx_nallocx!(rt_primes_8191bytes_8align_mallocx_nallocx, 8191, 8);
rt_alloc_layout_checked!(rt_primes_8191bytes_8align_alloc_layout_checked, 8191, 8);
rt_alloc_layout_unchecked!(rt_primes_8191bytes_8align_alloc_layout_unchecked, 8191, 8);
rt_alloc_excess_unused!(rt_primes_8191bytes_8align_alloc_excess_unused, 8191, 8);
rt_alloc_excess_used!(rt_primes_8191bytes_8align_alloc_excess_used, 8191, 8);
rt_realloc_naive!(rt_primes_8191bytes_8align_realloc_naive, 8191, 8);
rt_realloc!(rt_primes_8191bytes_8align_realloc, 8191, 8);
rt_realloc_excess_unused!(rt_primes_8191bytes_8align_realloc_excess_unused, 8191, 8);
rt_realloc_excess_used!(rt_primes_8191bytes_8align_realloc_excess_used, 8191, 8);

rt_calloc!(rt_primes_16381bytes_8align_calloc, 16381, 8);
rt_mallocx!(rt_primes_16381bytes_8align_mallocx, 16381, 8);
rt_mallocx_zeroed!(rt_primes_16381bytes_8align_mallocx_zeroed, 16381, 8);
rt_mallocx_nallocx!(rt_primes_16381bytes_8align_mallocx_nallocx, 16381, 8);
rt_alloc_layout_checked!(rt_primes_16381bytes_8align_alloc_layout_checked, 16381, 8);
rt_alloc_layout_unchecked!(rt_primes_16381bytes_8align_alloc_layout_unchecked, 16381, 8);
rt_alloc_excess_unused!(rt_primes_16381bytes_8align_alloc_excess_unused, 16381, 8);
rt_alloc_excess_used!(rt_primes_16381bytes_8align_alloc_excess_used, 16381, 8);
rt_realloc_naive!(rt_primes_16381bytes_8align_realloc_naive, 16381, 8);
rt_realloc!(rt_primes_16381bytes_8align_realloc, 16381, 8);
rt_realloc_excess_unused!(rt_primes_16381bytes_8align_realloc_excess_unused, 16381, 8);
rt_realloc_excess_used!(rt_primes_16381bytes_8align_realloc_excess_used, 16381, 8);

rt_calloc!(rt_primes_32749bytes_8align_calloc, 32749, 8);
rt_mallocx!(rt_primes_32749bytes_8align_mallocx, 32749, 8);
rt_mallocx_zeroed!(rt_primes_32749bytes_8align_mallocx_zeroed, 32749, 8);
rt_mallocx_nallocx!(rt_primes_32749bytes_8align_mallocx_nallocx, 32749, 8);
rt_alloc_layout_checked!(rt_primes_32749bytes_8align_alloc_layout_checked, 32749, 8);
rt_alloc_layout_unchecked!(rt_primes_32749bytes_8align_alloc_layout_unchecked, 32749, 8);
rt_alloc_excess_unused!(rt_primes_32749bytes_8align_alloc_excess_unused, 32749, 8);
rt_alloc_excess_used!(rt_primes_32749bytes_8align_alloc_excess_used, 32749, 8);
rt_realloc_naive!(rt_primes_32749bytes_8align_realloc_naive, 32749, 8);
rt_realloc!(rt_primes_32749bytes_8align_realloc, 32749, 8);
rt_realloc_excess_unused!(rt_primes_32749bytes_8align_realloc_excess_unused, 32749, 8);
rt_realloc_excess_used!(rt_primes_32749bytes_8align_realloc_excess_used, 32749, 8);

rt_calloc!(rt_primes_65537bytes_8align_calloc, 65537, 8);
rt_mallocx!(rt_primes_65537bytes_8align_mallocx, 65537, 8);
rt_mallocx_zeroed!(rt_primes_65537bytes_8align_mallocx_zeroed, 65537, 8);
rt_mallocx_nallocx!(rt_primes_65537bytes_8align_mallocx_nallocx, 65537, 8);
rt_alloc_layout_checked!(rt_primes_65537bytes_8align_alloc_layout_checked, 65537, 8);
rt_alloc_layout_unchecked!(rt_primes_65537bytes_8align_alloc_layout_unchecked, 65537, 8);
rt_alloc_excess_unused!(rt_primes_65537bytes_8align_alloc_excess_unused, 65537, 8);
rt_alloc_excess_used!(rt_primes_65537bytes_8align_alloc_excess_used, 65537, 8);
rt_realloc_naive!(rt_primes_65537bytes_8align_realloc_naive, 65537, 8);
rt_realloc!(rt_primes_65537bytes_8align_realloc, 65537, 8);
rt_realloc_excess_unused!(rt_primes_65537bytes_8align_realloc_excess_unused, 65537, 8);
rt_realloc_excess_used!(rt_primes_65537bytes_8align_realloc_excess_used, 65537, 8);

rt_calloc!(rt_primes_131071bytes_8align_calloc, 131071, 8);
rt_mallocx!(rt_primes_131071bytes_8align_mallocx, 131071, 8);
rt_mallocx_zeroed!(rt_primes_131071bytes_8align_mallocx_zeroed, 131071, 8);
rt_mallocx_nallocx!(rt_primes_131071bytes_8align_mallocx_nallocx, 131071, 8);
rt_alloc_layout_checked!(rt_primes_131071bytes_8align_alloc_layout_checked, 131071, 8);
rt_alloc_layout_unchecked!(
    rt_primes_131071bytes_8align_alloc_layout_unchecked,
    131071,
    8
);
rt_alloc_excess_unused!(rt_primes_131071bytes_8align_alloc_excess_unused, 131071, 8);
rt_alloc_excess_used!(rt_primes_131071bytes_8align_alloc_excess_used, 131071, 8);
rt_realloc_naive!(rt_primes_131071bytes_8align_realloc_naive, 131071, 8);
rt_realloc!(rt_primes_131071bytes_8align_realloc, 131071, 8);
rt_realloc_excess_unused!(
    rt_primes_131071bytes_8align_realloc_excess_unused,
    131071,
    8
);
rt_realloc_excess_used!(rt_primes_131071bytes_8align_realloc_excess_used, 131071, 8);

rt_calloc!(rt_primes_4194301bytes_8align_calloc, 4194301, 8);
rt_mallocx!(rt_primes_4194301bytes_8align_mallocx, 4194301, 8);
rt_mallocx_zeroed!(rt_primes_4194301bytes_8align_mallocx_zeroed, 4194301, 8);
rt_mallocx_nallocx!(rt_primes_4194301bytes_8align_mallocx_nallocx, 4194301, 8);
rt_alloc_layout_checked!(
    rt_primes_4194301bytes_8align_alloc_layout_checked,
    4194301,
    8
);
rt_alloc_layout_unchecked!(
    rt_primes_4194301bytes_8align_alloc_layout_unchecked,
    4194301,
    8
);
rt_alloc_excess_unused!(
    rt_primes_4194301bytes_8align_alloc_excess_unused,
    4194301,
    8
);
rt_alloc_excess_used!(rt_primes_4194301bytes_8align_alloc_excess_used, 4194301, 8);
rt_realloc_naive!(rt_primes_4194301bytes_8align_realloc_naive, 4194301, 8);
rt_realloc!(rt_primes_4194301bytes_8align_realloc, 4194301, 8);
rt_realloc_excess_unused!(
    rt_primes_4194301bytes_8align_realloc_excess_unused,
    4194301,
    8
);
rt_realloc_excess_used!(
    rt_primes_4194301bytes_8align_realloc_excess_used,
    4194301,
    8
);

// 16 bytes alignment

// Powers of two:
rt_calloc!(rt_pow2_1bytes_16align_calloc, 1, 16);
rt_mallocx!(rt_pow2_1bytes_16align_mallocx, 1, 16);
rt_mallocx_zeroed!(rt_pow2_1bytes_16align_mallocx_zeroed, 1, 16);
rt_mallocx_nallocx!(rt_pow2_1bytes_16align_mallocx_nallocx, 1, 16);
rt_alloc_layout_checked!(rt_pow2_1bytes_16align_alloc_layout_checked, 1, 16);
rt_alloc_layout_unchecked!(rt_pow2_1bytes_16align_alloc_layout_unchecked, 1, 16);
rt_alloc_excess_unused!(rt_pow2_1bytes_16align_alloc_excess_unused, 1, 16);
rt_alloc_excess_used!(rt_pow2_1bytes_16align_alloc_excess_used, 1, 16);
rt_realloc_naive!(rt_pow2_1bytes_16align_realloc_naive, 1, 16);
rt_realloc!(rt_pow2_1bytes_16align_realloc, 1, 16);
rt_realloc_excess_unused!(rt_pow2_1bytes_16align_realloc_excess_unused, 1, 16);
rt_realloc_excess_used!(rt_pow2_1bytes_16align_realloc_excess_used, 1, 16);

rt_calloc!(rt_pow2_2bytes_16align_calloc, 2, 16);
rt_mallocx!(rt_pow2_2bytes_16align_mallocx, 2, 16);
rt_mallocx_zeroed!(rt_pow2_2bytes_16align_mallocx_zeroed, 2, 16);
rt_mallocx_nallocx!(rt_pow2_2bytes_16align_mallocx_nallocx, 2, 16);
rt_alloc_layout_checked!(rt_pow2_2bytes_16align_alloc_layout_checked, 2, 16);
rt_alloc_layout_unchecked!(rt_pow2_2bytes_16align_alloc_layout_unchecked, 2, 16);
rt_alloc_excess_unused!(rt_pow2_2bytes_16align_alloc_excess_unused, 2, 16);
rt_alloc_excess_used!(rt_pow2_2bytes_16align_alloc_excess_used, 2, 16);
rt_realloc_naive!(rt_pow2_2bytes_16align_realloc_naive, 2, 16);
rt_realloc!(rt_pow2_2bytes_16align_realloc, 2, 16);
rt_realloc_excess_unused!(rt_pow2_2bytes_16align_realloc_excess_unused, 2, 16);
rt_realloc_excess_used!(rt_pow2_2bytes_16align_realloc_excess_used, 2, 16);

rt_calloc!(rt_pow2_4bytes_16align_calloc, 4, 16);
rt_mallocx!(rt_pow2_4bytes_16align_mallocx, 4, 16);
rt_mallocx_zeroed!(rt_pow2_4bytes_16align_mallocx_zeroed, 4, 16);
rt_mallocx_nallocx!(rt_pow2_4bytes_16align_mallocx_nallocx, 4, 16);
rt_alloc_layout_checked!(rt_pow2_4bytes_16align_alloc_layout_checked, 4, 16);
rt_alloc_layout_unchecked!(rt_pow2_4bytes_16align_alloc_layout_unchecked, 4, 16);
rt_alloc_excess_unused!(rt_pow2_4bytes_16align_alloc_excess_unused, 4, 16);
rt_alloc_excess_used!(rt_pow2_4bytes_16align_alloc_excess_used, 4, 16);
rt_realloc_naive!(rt_pow2_4bytes_16align_realloc_naive, 4, 16);
rt_realloc!(rt_pow2_4bytes_16align_realloc, 4, 16);
rt_realloc_excess_unused!(rt_pow2_4bytes_16align_realloc_excess_unused, 4, 16);
rt_realloc_excess_used!(rt_pow2_4bytes_16align_realloc_excess_used, 4, 16);

rt_calloc!(rt_pow2_8bytes_16align_calloc, 8, 16);
rt_mallocx!(rt_pow2_8bytes_16align_mallocx, 8, 16);
rt_mallocx_zeroed!(rt_pow2_8bytes_16align_mallocx_zeroed, 8, 16);
rt_mallocx_nallocx!(rt_pow2_8bytes_16align_mallocx_nallocx, 8, 16);
rt_alloc_layout_checked!(rt_pow2_8bytes_16align_alloc_layout_checked, 8, 16);
rt_alloc_layout_unchecked!(rt_pow2_8bytes_16align_alloc_layout_unchecked, 8, 16);
rt_alloc_excess_unused!(rt_pow2_8bytes_16align_alloc_excess_unused, 8, 16);
rt_alloc_excess_used!(rt_pow2_8bytes_16align_alloc_excess_used, 8, 16);
rt_realloc_naive!(rt_pow2_8bytes_16align_realloc_naive, 8, 16);
rt_realloc!(rt_pow2_8bytes_16align_realloc, 8, 16);
rt_realloc_excess_unused!(rt_pow2_8bytes_16align_realloc_excess_unused, 8, 16);
rt_realloc_excess_used!(rt_pow2_8bytes_16align_realloc_excess_used, 8, 16);

rt_calloc!(rt_pow2_16bytes_16align_calloc, 16, 16);
rt_mallocx!(rt_pow2_16bytes_16align_mallocx, 16, 16);
rt_mallocx_zeroed!(rt_pow2_16bytes_16align_mallocx_zeroed, 16, 16);
rt_mallocx_nallocx!(rt_pow2_16bytes_16align_mallocx_nallocx, 16, 16);
rt_alloc_layout_checked!(rt_pow2_16bytes_16align_alloc_layout_checked, 16, 16);
rt_alloc_layout_unchecked!(rt_pow2_16bytes_16align_alloc_layout_unchecked, 16, 16);
rt_alloc_excess_unused!(rt_pow2_16bytes_16align_alloc_excess_unused, 16, 16);
rt_alloc_excess_used!(rt_pow2_16bytes_16align_alloc_excess_used, 16, 16);
rt_realloc_naive!(rt_pow2_16bytes_16align_realloc_naive, 16, 16);
rt_realloc!(rt_pow2_16bytes_16align_realloc, 16, 16);
rt_realloc_excess_unused!(rt_pow2_16bytes_16align_realloc_excess_unused, 16, 16);
rt_realloc_excess_used!(rt_pow2_16bytes_16align_realloc_excess_used, 16, 16);

rt_calloc!(rt_pow2_32bytes_16align_calloc, 32, 16);
rt_mallocx!(rt_pow2_32bytes_16align_mallocx, 32, 16);
rt_mallocx_zeroed!(rt_pow2_32bytes_16align_mallocx_zeroed, 32, 16);
rt_mallocx_nallocx!(rt_pow2_32bytes_16align_mallocx_nallocx, 32, 16);
rt_alloc_layout_checked!(rt_pow2_32bytes_16align_alloc_layout_checked, 32, 16);
rt_alloc_layout_unchecked!(rt_pow2_32bytes_16align_alloc_layout_unchecked, 32, 16);
rt_alloc_excess_unused!(rt_pow2_32bytes_16align_alloc_excess_unused, 32, 16);
rt_alloc_excess_used!(rt_pow2_32bytes_16align_alloc_excess_used, 32, 16);
rt_realloc_naive!(rt_pow2_32bytes_16align_realloc_naive, 32, 16);
rt_realloc!(rt_pow2_32bytes_16align_realloc, 32, 16);
rt_realloc_excess_unused!(rt_pow2_32bytes_16align_realloc_excess_unused, 32, 16);
rt_realloc_excess_used!(rt_pow2_32bytes_16align_realloc_excess_used, 32, 16);

rt_calloc!(rt_pow2_64bytes_16align_calloc, 64, 16);
rt_mallocx!(rt_pow2_64bytes_16align_mallocx, 64, 16);
rt_mallocx_zeroed!(rt_pow2_64bytes_16align_mallocx_zeroed, 64, 16);
rt_mallocx_nallocx!(rt_pow2_64bytes_16align_mallocx_nallocx, 64, 16);
rt_alloc_layout_checked!(rt_pow2_64bytes_16align_alloc_layout_checked, 64, 16);
rt_alloc_layout_unchecked!(rt_pow2_64bytes_16align_alloc_layout_unchecked, 64, 16);
rt_alloc_excess_unused!(rt_pow2_64bytes_16align_alloc_excess_unused, 64, 16);
rt_alloc_excess_used!(rt_pow2_64bytes_16align_alloc_excess_used, 64, 16);
rt_realloc_naive!(rt_pow2_64bytes_16align_realloc_naive, 64, 16);
rt_realloc!(rt_pow2_64bytes_16align_realloc, 64, 16);
rt_realloc_excess_unused!(rt_pow2_64bytes_16align_realloc_excess_unused, 64, 16);
rt_realloc_excess_used!(rt_pow2_64bytes_16align_realloc_excess_used, 64, 16);

rt_calloc!(rt_pow2_128bytes_16align_calloc, 128, 16);
rt_mallocx!(rt_pow2_128bytes_16align_mallocx, 128, 16);
rt_mallocx_zeroed!(rt_pow2_128bytes_16align_mallocx_zeroed, 128, 16);
rt_mallocx_nallocx!(rt_pow2_128bytes_16align_mallocx_nallocx, 128, 16);
rt_alloc_layout_checked!(rt_pow2_128bytes_16align_alloc_layout_checked, 128, 16);
rt_alloc_layout_unchecked!(rt_pow2_128bytes_16align_alloc_layout_unchecked, 128, 16);
rt_alloc_excess_unused!(rt_pow2_128bytes_16align_alloc_excess_unused, 128, 16);
rt_alloc_excess_used!(rt_pow2_128bytes_16align_alloc_excess_used, 128, 16);
rt_realloc_naive!(rt_pow2_128bytes_16align_realloc_naive, 128, 16);
rt_realloc!(rt_pow2_128bytes_16align_realloc, 128, 16);
rt_realloc_excess_unused!(rt_pow2_128bytes_16align_realloc_excess_unused, 128, 16);
rt_realloc_excess_used!(rt_pow2_128bytes_16align_realloc_excess_used, 128, 16);

rt_calloc!(rt_pow2_256bytes_16align_calloc, 256, 16);
rt_mallocx!(rt_pow2_256bytes_16align_mallocx, 256, 16);
rt_mallocx_zeroed!(rt_pow2_256bytes_16align_mallocx_zeroed, 256, 16);
rt_mallocx_nallocx!(rt_pow2_256bytes_16align_mallocx_nallocx, 256, 16);
rt_alloc_layout_checked!(rt_pow2_256bytes_16align_alloc_layout_checked, 256, 16);
rt_alloc_layout_unchecked!(rt_pow2_256bytes_16align_alloc_layout_unchecked, 256, 16);
rt_alloc_excess_unused!(rt_pow2_256bytes_16align_alloc_excess_unused, 256, 16);
rt_alloc_excess_used!(rt_pow2_256bytes_16align_alloc_excess_used, 256, 16);
rt_realloc_naive!(rt_pow2_256bytes_16align_realloc_naive, 256, 16);
rt_realloc!(rt_pow2_256bytes_16align_realloc, 256, 16);
rt_realloc_excess_unused!(rt_pow2_256bytes_16align_realloc_excess_unused, 256, 16);
rt_realloc_excess_used!(rt_pow2_256bytes_16align_realloc_excess_used, 256, 16);

rt_calloc!(rt_pow2_512bytes_16align_calloc, 512, 16);
rt_mallocx!(rt_pow2_512bytes_16align_mallocx, 512, 16);
rt_mallocx_zeroed!(rt_pow2_512bytes_16align_mallocx_zeroed, 512, 16);
rt_mallocx_nallocx!(rt_pow2_512bytes_16align_mallocx_nallocx, 512, 16);
rt_alloc_layout_checked!(rt_pow2_512bytes_16align_alloc_layout_checked, 512, 16);
rt_alloc_layout_unchecked!(rt_pow2_512bytes_16align_alloc_layout_unchecked, 512, 16);
rt_alloc_excess_unused!(rt_pow2_512bytes_16align_alloc_excess_unused, 512, 16);
rt_alloc_excess_used!(rt_pow2_512bytes_16align_alloc_excess_used, 512, 16);
rt_realloc_naive!(rt_pow2_512bytes_16align_realloc_naive, 512, 16);
rt_realloc!(rt_pow2_512bytes_16align_realloc, 512, 16);
rt_realloc_excess_unused!(rt_pow2_512bytes_16align_realloc_excess_unused, 512, 16);
rt_realloc_excess_used!(rt_pow2_512bytes_16align_realloc_excess_used, 512, 16);

rt_calloc!(rt_pow2_1024bytes_16align_calloc, 1024, 16);
rt_mallocx!(rt_pow2_1024bytes_16align_mallocx, 1024, 16);
rt_mallocx_zeroed!(rt_pow2_1024bytes_16align_mallocx_zeroed, 1024, 16);
rt_mallocx_nallocx!(rt_pow2_1024bytes_16align_mallocx_nallocx, 1024, 16);
rt_alloc_layout_checked!(rt_pow2_1024bytes_16align_alloc_layout_checked, 1024, 16);
rt_alloc_layout_unchecked!(rt_pow2_1024bytes_16align_alloc_layout_unchecked, 1024, 16);
rt_alloc_excess_unused!(rt_pow2_1024bytes_16align_alloc_excess_unused, 1024, 16);
rt_alloc_excess_used!(rt_pow2_1024bytes_16align_alloc_excess_used, 1024, 16);
rt_realloc_naive!(rt_pow2_1024bytes_16align_realloc_naive, 1024, 16);
rt_realloc!(rt_pow2_1024bytes_16align_realloc, 1024, 16);
rt_realloc_excess_unused!(rt_pow2_1024bytes_16align_realloc_excess_unused, 1024, 16);
rt_realloc_excess_used!(rt_pow2_1024bytes_16align_realloc_excess_used, 1024, 16);

rt_calloc!(rt_pow2_2048bytes_16align_calloc, 2048, 16);
rt_mallocx!(rt_pow2_2048bytes_16align_mallocx, 2048, 16);
rt_mallocx_zeroed!(rt_pow2_2048bytes_16align_mallocx_zeroed, 2048, 16);
rt_mallocx_nallocx!(rt_pow2_2048bytes_16align_mallocx_nallocx, 2048, 16);
rt_alloc_layout_checked!(rt_pow2_2048bytes_16align_alloc_layout_checked, 2048, 16);
rt_alloc_layout_unchecked!(rt_pow2_2048bytes_16align_alloc_layout_unchecked, 2048, 16);
rt_alloc_excess_unused!(rt_pow2_2048bytes_16align_alloc_excess_unused, 2048, 16);
rt_alloc_excess_used!(rt_pow2_2048bytes_16align_alloc_excess_used, 2048, 16);
rt_realloc_naive!(rt_pow2_2048bytes_16align_realloc_naive, 2048, 16);
rt_realloc!(rt_pow2_2048bytes_16align_realloc, 2048, 16);
rt_realloc_excess_unused!(rt_pow2_2048bytes_16align_realloc_excess_unused, 2048, 16);
rt_realloc_excess_used!(rt_pow2_2048bytes_16align_realloc_excess_used, 2048, 16);

rt_calloc!(rt_pow2_4096bytes_16align_calloc, 4096, 16);
rt_mallocx!(rt_pow2_4096bytes_16align_mallocx, 4096, 16);
rt_mallocx_zeroed!(rt_pow2_4096bytes_16align_mallocx_zeroed, 4096, 16);
rt_mallocx_nallocx!(rt_pow2_4096bytes_16align_mallocx_nallocx, 4096, 16);
rt_alloc_layout_checked!(rt_pow2_4096bytes_16align_alloc_layout_checked, 4096, 16);
rt_alloc_layout_unchecked!(rt_pow2_4096bytes_16align_alloc_layout_unchecked, 4096, 16);
rt_alloc_excess_unused!(rt_pow2_4096bytes_16align_alloc_excess_unused, 4096, 16);
rt_alloc_excess_used!(rt_pow2_4096bytes_16align_alloc_excess_used, 4096, 16);
rt_realloc_naive!(rt_pow2_4096bytes_16align_realloc_naive, 4096, 16);
rt_realloc!(rt_pow2_4096bytes_16align_realloc, 4096, 16);
rt_realloc_excess_unused!(rt_pow2_4096bytes_16align_realloc_excess_unused, 4096, 16);
rt_realloc_excess_used!(rt_pow2_4096bytes_16align_realloc_excess_used, 4096, 16);

rt_calloc!(rt_pow2_8192bytes_16align_calloc, 8192, 16);
rt_mallocx!(rt_pow2_8192bytes_16align_mallocx, 8192, 16);
rt_mallocx_zeroed!(rt_pow2_8192bytes_16align_mallocx_zeroed, 8192, 16);
rt_mallocx_nallocx!(rt_pow2_8192bytes_16align_mallocx_nallocx, 8192, 16);
rt_alloc_layout_checked!(rt_pow2_8192bytes_16align_alloc_layout_checked, 8192, 16);
rt_alloc_layout_unchecked!(rt_pow2_8192bytes_16align_alloc_layout_unchecked, 8192, 16);
rt_alloc_excess_unused!(rt_pow2_8192bytes_16align_alloc_excess_unused, 8192, 16);
rt_alloc_excess_used!(rt_pow2_8192bytes_16align_alloc_excess_used, 8192, 16);
rt_realloc_naive!(rt_pow2_8192bytes_16align_realloc_naive, 8192, 16);
rt_realloc!(rt_pow2_8192bytes_16align_realloc, 8192, 16);
rt_realloc_excess_unused!(rt_pow2_8192bytes_16align_realloc_excess_unused, 8192, 16);
rt_realloc_excess_used!(rt_pow2_8192bytes_16align_realloc_excess_used, 8192, 16);

rt_calloc!(rt_pow2_16384bytes_16align_calloc, 16384, 16);
rt_mallocx!(rt_pow2_16384bytes_16align_mallocx, 16384, 16);
rt_mallocx_zeroed!(rt_pow2_16384bytes_16align_mallocx_zeroed, 16384, 16);
rt_mallocx_nallocx!(rt_pow2_16384bytes_16align_mallocx_nallocx, 16384, 16);
rt_alloc_layout_checked!(rt_pow2_16384bytes_16align_alloc_layout_checked, 16384, 16);
rt_alloc_layout_unchecked!(rt_pow2_16384bytes_16align_alloc_layout_unchecked, 16384, 16);
rt_alloc_excess_unused!(rt_pow2_16384bytes_16align_alloc_excess_unused, 16384, 16);
rt_alloc_excess_used!(rt_pow2_16384bytes_16align_alloc_excess_used, 16384, 16);
rt_realloc_naive!(rt_pow2_16384bytes_16align_realloc_naive, 16384, 16);
rt_realloc!(rt_pow2_16384bytes_16align_realloc, 16384, 16);
rt_realloc_excess_unused!(rt_pow2_16384bytes_16align_realloc_excess_unused, 16384, 16);
rt_realloc_excess_used!(rt_pow2_16384bytes_16align_realloc_excess_used, 16384, 16);

rt_calloc!(rt_pow2_32768bytes_16align_calloc, 32768, 16);
rt_mallocx!(rt_pow2_32768bytes_16align_mallocx, 32768, 16);
rt_mallocx_zeroed!(rt_pow2_32768bytes_16align_mallocx_zeroed, 32768, 16);
rt_mallocx_nallocx!(rt_pow2_32768bytes_16align_mallocx_nallocx, 32768, 16);
rt_alloc_layout_checked!(rt_pow2_32768bytes_16align_alloc_layout_checked, 32768, 16);
rt_alloc_layout_unchecked!(rt_pow2_32768bytes_16align_alloc_layout_unchecked, 32768, 16);
rt_alloc_excess_unused!(rt_pow2_32768bytes_16align_alloc_excess_unused, 32768, 16);
rt_alloc_excess_used!(rt_pow2_32768bytes_16align_alloc_excess_used, 32768, 16);
rt_realloc_naive!(rt_pow2_32768bytes_16align_realloc_naive, 32768, 16);
rt_realloc!(rt_pow2_32768bytes_16align_realloc, 32768, 16);
rt_realloc_excess_unused!(rt_pow2_32768bytes_16align_realloc_excess_unused, 32768, 16);
rt_realloc_excess_used!(rt_pow2_32768bytes_16align_realloc_excess_used, 32768, 16);

rt_calloc!(rt_pow2_65536bytes_16align_calloc, 65536, 16);
rt_mallocx!(rt_pow2_65536bytes_16align_mallocx, 65536, 16);
rt_mallocx_zeroed!(rt_pow2_65536bytes_16align_mallocx_zeroed, 65536, 16);
rt_mallocx_nallocx!(rt_pow2_65536bytes_16align_mallocx_nallocx, 65536, 16);
rt_alloc_layout_checked!(rt_pow2_65536bytes_16align_alloc_layout_checked, 65536, 16);
rt_alloc_layout_unchecked!(rt_pow2_65536bytes_16align_alloc_layout_unchecked, 65536, 16);
rt_alloc_excess_unused!(rt_pow2_65536bytes_16align_alloc_excess_unused, 65536, 16);
rt_alloc_excess_used!(rt_pow2_65536bytes_16align_alloc_excess_used, 65536, 16);
rt_realloc_naive!(rt_pow2_65536bytes_16align_realloc_naive, 65536, 16);
rt_realloc!(rt_pow2_65536bytes_16align_realloc, 65536, 16);
rt_realloc_excess_unused!(rt_pow2_65536bytes_16align_realloc_excess_unused, 65536, 16);
rt_realloc_excess_used!(rt_pow2_65536bytes_16align_realloc_excess_used, 65536, 16);

rt_calloc!(rt_pow2_131072bytes_16align_calloc, 131072, 16);
rt_mallocx!(rt_pow2_131072bytes_16align_mallocx, 131072, 16);
rt_mallocx_zeroed!(rt_pow2_131072bytes_16align_mallocx_zeroed, 131072, 16);
rt_mallocx_nallocx!(rt_pow2_131072bytes_16align_mallocx_nallocx, 131072, 16);
rt_alloc_layout_checked!(rt_pow2_131072bytes_16align_alloc_layout_checked, 131072, 16);
rt_alloc_layout_unchecked!(
    rt_pow2_131072bytes_16align_alloc_layout_unchecked,
    131072,
    16
);
rt_alloc_excess_unused!(rt_pow2_131072bytes_16align_alloc_excess_unused, 131072, 16);
rt_alloc_excess_used!(rt_pow2_131072bytes_16align_alloc_excess_used, 131072, 16);
rt_realloc_naive!(rt_pow2_131072bytes_16align_realloc_naive, 131072, 16);
rt_realloc!(rt_pow2_131072bytes_16align_realloc, 131072, 16);
rt_realloc_excess_unused!(
    rt_pow2_131072bytes_16align_realloc_excess_unused,
    131072,
    16
);
rt_realloc_excess_used!(rt_pow2_131072bytes_16align_realloc_excess_used, 131072, 16);

rt_calloc!(rt_pow2_4194304bytes_16align_calloc, 4194304, 16);
rt_mallocx!(rt_pow2_4194304bytes_16align_mallocx, 4194304, 16);
rt_mallocx_zeroed!(rt_pow2_4194304bytes_16align_mallocx_zeroed, 4194304, 16);
rt_mallocx_nallocx!(rt_pow2_4194304bytes_16align_mallocx_nallocx, 4194304, 16);
rt_alloc_layout_checked!(
    rt_pow2_4194304bytes_16align_alloc_layout_checked,
    4194304,
    16
);
rt_alloc_layout_unchecked!(
    rt_pow2_4194304bytes_16align_alloc_layout_unchecked,
    4194304,
    16
);
rt_alloc_excess_unused!(
    rt_pow2_4194304bytes_16align_alloc_excess_unused,
    4194304,
    16
);
rt_alloc_excess_used!(rt_pow2_4194304bytes_16align_alloc_excess_used, 4194304, 16);
rt_realloc_naive!(rt_pow2_4194304bytes_16align_realloc_naive, 4194304, 16);
rt_realloc!(rt_pow2_4194304bytes_16align_realloc, 4194304, 16);
rt_realloc_excess_unused!(
    rt_pow2_4194304bytes_16align_realloc_excess_unused,
    4194304,
    16
);
rt_realloc_excess_used!(
    rt_pow2_4194304bytes_16align_realloc_excess_used,
    4194304,
    16
);

// Even
rt_calloc!(rt_even_10bytes_16align_calloc, 10, 16);
rt_mallocx!(rt_even_10bytes_16align_mallocx, 10, 16);
rt_mallocx_zeroed!(rt_even_10bytes_16align_mallocx_zeroed, 10, 16);
rt_mallocx_nallocx!(rt_even_10bytes_16align_mallocx_nallocx, 10, 16);
rt_alloc_layout_checked!(rt_even_10bytes_16align_alloc_layout_checked, 10, 16);
rt_alloc_layout_unchecked!(rt_even_10bytes_16align_alloc_layout_unchecked, 10, 16);
rt_alloc_excess_unused!(rt_even_10bytes_16align_alloc_excess_unused, 10, 16);
rt_alloc_excess_used!(rt_even_10bytes_16align_alloc_excess_used, 10, 16);
rt_realloc_naive!(rt_even_10bytes_16align_realloc_naive, 10, 16);
rt_realloc!(rt_even_10bytes_16align_realloc, 10, 16);
rt_realloc_excess_unused!(rt_even_10bytes_16align_realloc_excess_unused, 10, 16);
rt_realloc_excess_used!(rt_even_10bytes_16align_realloc_excess_used, 10, 16);

rt_calloc!(rt_even_100bytes_16align_calloc, 100, 16);
rt_mallocx!(rt_even_100bytes_16align_mallocx, 100, 16);
rt_mallocx_zeroed!(rt_even_100bytes_16align_mallocx_zeroed, 100, 16);
rt_mallocx_nallocx!(rt_even_100bytes_16align_mallocx_nallocx, 100, 16);
rt_alloc_layout_checked!(rt_even_100bytes_16align_alloc_layout_checked, 100, 16);
rt_alloc_layout_unchecked!(rt_even_100bytes_16align_alloc_layout_unchecked, 100, 16);
rt_alloc_excess_unused!(rt_even_100bytes_16align_alloc_excess_unused, 100, 16);
rt_alloc_excess_used!(rt_even_100bytes_16align_alloc_excess_used, 100, 16);
rt_realloc_naive!(rt_even_100bytes_16align_realloc_naive, 100, 16);
rt_realloc!(rt_even_100bytes_16align_realloc, 100, 16);
rt_realloc_excess_unused!(rt_even_100bytes_16align_realloc_excess_unused, 100, 16);
rt_realloc_excess_used!(rt_even_100bytes_16align_realloc_excess_used, 100, 16);

rt_calloc!(rt_even_1000bytes_16align_calloc, 1000, 16);
rt_mallocx!(rt_even_1000bytes_16align_mallocx, 1000, 16);
rt_mallocx_zeroed!(rt_even_1000bytes_16align_mallocx_zeroed, 1000, 16);
rt_mallocx_nallocx!(rt_even_1000bytes_16align_mallocx_nallocx, 1000, 16);
rt_alloc_layout_checked!(rt_even_1000bytes_16align_alloc_layout_checked, 1000, 16);
rt_alloc_layout_unchecked!(rt_even_1000bytes_16align_alloc_layout_unchecked, 1000, 16);
rt_alloc_excess_unused!(rt_even_1000bytes_16align_alloc_excess_unused, 1000, 16);
rt_alloc_excess_used!(rt_even_1000bytes_16align_alloc_excess_used, 1000, 16);
rt_realloc_naive!(rt_even_1000bytes_16align_realloc_naive, 1000, 16);
rt_realloc!(rt_even_1000bytes_16align_realloc, 1000, 16);
rt_realloc_excess_unused!(rt_even_1000bytes_16align_realloc_excess_unused, 1000, 16);
rt_realloc_excess_used!(rt_even_1000bytes_16align_realloc_excess_used, 1000, 16);

rt_calloc!(rt_even_10000bytes_16align_calloc, 10000, 16);
rt_mallocx!(rt_even_10000bytes_16align_mallocx, 10000, 16);
rt_mallocx_zeroed!(rt_even_10000bytes_16align_mallocx_zeroed, 10000, 16);
rt_mallocx_nallocx!(rt_even_10000bytes_16align_mallocx_nallocx, 10000, 16);
rt_alloc_layout_checked!(rt_even_10000bytes_16align_alloc_layout_checked, 10000, 16);
rt_alloc_layout_unchecked!(rt_even_10000bytes_16align_alloc_layout_unchecked, 10000, 16);
rt_alloc_excess_unused!(rt_even_10000bytes_16align_alloc_excess_unused, 10000, 16);
rt_alloc_excess_used!(rt_even_10000bytes_16align_alloc_excess_used, 10000, 16);
rt_realloc_naive!(rt_even_10000bytes_16align_realloc_naive, 10000, 16);
rt_realloc!(rt_even_10000bytes_16align_realloc, 10000, 16);
rt_realloc_excess_unused!(rt_even_10000bytes_16align_realloc_excess_unused, 10000, 16);
rt_realloc_excess_used!(rt_even_10000bytes_16align_realloc_excess_used, 10000, 16);

rt_calloc!(rt_even_100000bytes_16align_calloc, 100000, 16);
rt_mallocx!(rt_even_100000bytes_16align_mallocx, 100000, 16);
rt_mallocx_zeroed!(rt_even_100000bytes_16align_mallocx_zeroed, 100000, 16);
rt_mallocx_nallocx!(rt_even_100000bytes_16align_mallocx_nallocx, 100000, 16);
rt_alloc_layout_checked!(rt_even_100000bytes_16align_alloc_layout_checked, 100000, 16);
rt_alloc_layout_unchecked!(
    rt_even_100000bytes_16align_alloc_layout_unchecked,
    100000,
    16
);
rt_alloc_excess_unused!(rt_even_100000bytes_16align_alloc_excess_unused, 100000, 16);
rt_alloc_excess_used!(rt_even_100000bytes_16align_alloc_excess_used, 100000, 16);
rt_realloc_naive!(rt_even_100000bytes_16align_realloc_naive, 100000, 16);
rt_realloc!(rt_even_100000bytes_16align_realloc, 100000, 16);
rt_realloc_excess_unused!(
    rt_even_100000bytes_16align_realloc_excess_unused,
    100000,
    16
);
rt_realloc_excess_used!(rt_even_100000bytes_16align_realloc_excess_used, 100000, 16);

rt_calloc!(rt_even_1000000bytes_16align_calloc, 1000000, 16);
rt_mallocx!(rt_even_1000000bytes_16align_mallocx, 1000000, 16);
rt_mallocx_zeroed!(rt_even_1000000bytes_16align_mallocx_zeroed, 1000000, 16);
rt_mallocx_nallocx!(rt_even_1000000bytes_16align_mallocx_nallocx, 1000000, 16);
rt_alloc_layout_checked!(
    rt_even_1000000bytes_16align_alloc_layout_checked,
    1000000,
    16
);
rt_alloc_layout_unchecked!(
    rt_even_1000000bytes_16align_alloc_layout_unchecked,
    1000000,
    16
);
rt_alloc_excess_unused!(
    rt_even_1000000bytes_16align_alloc_excess_unused,
    1000000,
    16
);
rt_alloc_excess_used!(rt_even_1000000bytes_16align_alloc_excess_used, 1000000, 16);
rt_realloc_naive!(rt_even_1000000bytes_16align_realloc_naive, 1000000, 16);
rt_realloc!(rt_even_1000000bytes_16align_realloc, 1000000, 16);
rt_realloc_excess_unused!(
    rt_even_1000000bytes_16align_realloc_excess_unused,
    1000000,
    16
);
rt_realloc_excess_used!(
    rt_even_1000000bytes_16align_realloc_excess_used,
    1000000,
    16
);

// Odd:
rt_calloc!(rt_odd_10bytes_16align_calloc, 10 - 1, 16);
rt_mallocx!(rt_odd_10bytes_16align_mallocx, 10 - 1, 16);
rt_mallocx_zeroed!(rt_odd_10bytes_16align_mallocx_zeroed, 10 - 1, 16);
rt_mallocx_nallocx!(rt_odd_10bytes_16align_mallocx_nallocx, 10 - 1, 16);
rt_alloc_layout_checked!(rt_odd_10bytes_16align_alloc_layout_checked, 10 - 1, 16);
rt_alloc_layout_unchecked!(rt_odd_10bytes_16align_alloc_layout_unchecked, 10 - 1, 16);
rt_alloc_excess_unused!(rt_odd_10bytes_16align_alloc_excess_unused, 10 - 1, 16);
rt_alloc_excess_used!(rt_odd_10bytes_16align_alloc_excess_used, 10 - 1, 16);
rt_realloc_naive!(rt_odd_10bytes_16align_realloc_naive, 10 - 1, 16);
rt_realloc!(rt_odd_10bytes_16align_realloc, 10 - 1, 16);
rt_realloc_excess_unused!(rt_odd_10bytes_16align_realloc_excess_unused, 10 - 1, 16);
rt_realloc_excess_used!(rt_odd_10bytes_16align_realloc_excess_used, 10 - 1, 16);

rt_calloc!(rt_odd_100bytes_16align_calloc, 100 - 1, 16);
rt_mallocx!(rt_odd_100bytes_16align_mallocx, 100 - 1, 16);
rt_mallocx_zeroed!(rt_odd_100bytes_16align_mallocx_zeroed, 100 - 1, 16);
rt_mallocx_nallocx!(rt_odd_100bytes_16align_mallocx_nallocx, 100 - 1, 16);
rt_alloc_layout_checked!(rt_odd_100bytes_16align_alloc_layout_checked, 100 - 1, 16);
rt_alloc_layout_unchecked!(rt_odd_100bytes_16align_alloc_layout_unchecked, 100 - 1, 16);
rt_alloc_excess_unused!(rt_odd_100bytes_16align_alloc_excess_unused, 100 - 1, 16);
rt_alloc_excess_used!(rt_odd_100bytes_16align_alloc_excess_used, 100 - 1, 16);
rt_realloc_naive!(rt_odd_100bytes_16align_realloc_naive, 100 - 1, 16);
rt_realloc!(rt_odd_100bytes_16align_realloc, 100 - 1, 16);
rt_realloc_excess_unused!(rt_odd_100bytes_16align_realloc_excess_unused, 100 - 1, 16);
rt_realloc_excess_used!(rt_odd_100bytes_16align_realloc_excess_used, 100 - 1, 16);

rt_calloc!(rt_odd_1000bytes_16align_calloc, 1000 - 1, 16);
rt_mallocx!(rt_odd_1000bytes_16align_mallocx, 1000 - 1, 16);
rt_mallocx_zeroed!(rt_odd_1000bytes_16align_mallocx_zeroed, 1000 - 1, 16);
rt_mallocx_nallocx!(rt_odd_1000bytes_16align_mallocx_nallocx, 1000 - 1, 16);
rt_alloc_layout_checked!(rt_odd_1000bytes_16align_alloc_layout_checked, 1000 - 1, 16);
rt_alloc_layout_unchecked!(
    rt_odd_1000bytes_16align_alloc_layout_unchecked,
    1000 - 1,
    16
);
rt_alloc_excess_unused!(rt_odd_1000bytes_16align_alloc_excess_unused, 1000 - 1, 16);
rt_alloc_excess_used!(rt_odd_1000bytes_16align_alloc_excess_used, 1000 - 1, 16);
rt_realloc_naive!(rt_odd_1000bytes_16align_realloc_naive, 1000 - 1, 16);
rt_realloc!(rt_odd_1000bytes_16align_realloc, 1000 - 1, 16);
rt_realloc_excess_unused!(rt_odd_1000bytes_16align_realloc_excess_unused, 1000 - 1, 16);
rt_realloc_excess_used!(rt_odd_1000bytes_16align_realloc_excess_used, 1000 - 1, 16);

rt_calloc!(rt_odd_10000bytes_16align_calloc, 10000 - 1, 16);
rt_mallocx!(rt_odd_10000bytes_16align_mallocx, 10000 - 1, 16);
rt_mallocx_zeroed!(rt_odd_10000bytes_16align_mallocx_zeroed, 10000 - 1, 16);
rt_mallocx_nallocx!(rt_odd_10000bytes_16align_mallocx_nallocx, 10000 - 1, 16);
rt_alloc_layout_checked!(
    rt_odd_10000bytes_16align_alloc_layout_checked,
    10000 - 1,
    16
);
rt_alloc_layout_unchecked!(
    rt_odd_10000bytes_16align_alloc_layout_unchecked,
    10000 - 1,
    16
);
rt_alloc_excess_unused!(rt_odd_10000bytes_16align_alloc_excess_unused, 10000 - 1, 16);
rt_alloc_excess_used!(rt_odd_10000bytes_16align_alloc_excess_used, 10000 - 1, 16);
rt_realloc_naive!(rt_odd_10000bytes_16align_realloc_naive, 10000 - 1, 16);
rt_realloc!(rt_odd_10000bytes_16align_realloc, 10000 - 1, 16);
rt_realloc_excess_unused!(
    rt_odd_10000bytes_16align_realloc_excess_unused,
    10000 - 1,
    16
);
rt_realloc_excess_used!(rt_odd_10000bytes_16align_realloc_excess_used, 10000 - 1, 16);

rt_calloc!(rt_odd_100000bytes_16align_calloc, 100000 - 1, 16);
rt_mallocx!(rt_odd_100000bytes_16align_mallocx, 100000 - 1, 16);
rt_mallocx_zeroed!(rt_odd_100000bytes_16align_mallocx_zeroed, 100000 - 1, 16);
rt_mallocx_nallocx!(rt_odd_100000bytes_16align_mallocx_nallocx, 100000 - 1, 16);
rt_alloc_layout_checked!(
    rt_odd_100000bytes_16align_alloc_layout_checked,
    100000 - 1,
    16
);
rt_alloc_layout_unchecked!(
    rt_odd_100000bytes_16align_alloc_layout_unchecked,
    100000 - 1,
    16
);
rt_alloc_excess_unused!(
    rt_odd_100000bytes_16align_alloc_excess_unused,
    100000 - 1,
    16
);
rt_alloc_excess_used!(rt_odd_100000bytes_16align_alloc_excess_used, 100000 - 1, 16);
rt_realloc_naive!(rt_odd_100000bytes_16align_realloc_naive, 100000 - 1, 16);
rt_realloc!(rt_odd_100000bytes_16align_realloc, 100000 - 1, 16);
rt_realloc_excess_unused!(
    rt_odd_100000bytes_16align_realloc_excess_unused,
    100000 - 1,
    16
);
rt_realloc_excess_used!(
    rt_odd_100000bytes_16align_realloc_excess_used,
    100000 - 1,
    16
);

rt_calloc!(rt_odd_1000000bytes_16align_calloc, 1000000 - 1, 16);
rt_mallocx!(rt_odd_1000000bytes_16align_mallocx, 1000000 - 1, 16);
rt_mallocx_zeroed!(rt_odd_1000000bytes_16align_mallocx_zeroed, 1000000 - 1, 16);
rt_mallocx_nallocx!(rt_odd_1000000bytes_16align_mallocx_nallocx, 1000000 - 1, 16);
rt_alloc_layout_checked!(
    rt_odd_1000000bytes_16align_alloc_layout_checked,
    1000000 - 1,
    16
);
rt_alloc_layout_unchecked!(
    rt_odd_1000000bytes_16align_alloc_layout_unchecked,
    1000000 - 1,
    16
);
rt_alloc_excess_unused!(
    rt_odd_1000000bytes_16align_alloc_excess_unused,
    1000000 - 1,
    16
);
rt_alloc_excess_used!(
    rt_odd_1000000bytes_16align_alloc_excess_used,
    1000000 - 1,
    16
);
rt_realloc_naive!(rt_odd_1000000bytes_16align_realloc_naive, 1000000 - 1, 16);
rt_realloc!(rt_odd_1000000bytes_16align_realloc, 1000000 - 1, 16);
rt_realloc_excess_unused!(
    rt_odd_1000000bytes_16align_realloc_excess_unused,
    1000000 - 1,
    16
);
rt_realloc_excess_used!(
    rt_odd_1000000bytes_16align_realloc_excess_used,
    1000000 - 1,
    16
);

// primes
rt_calloc!(rt_primes_3bytes_16align_calloc, 3, 16);
rt_mallocx!(rt_primes_3bytes_16align_mallocx, 3, 16);
rt_mallocx_zeroed!(rt_primes_3bytes_16align_mallocx_zeroed, 3, 16);
rt_mallocx_nallocx!(rt_primes_3bytes_16align_mallocx_nallocx, 3, 16);
rt_alloc_layout_checked!(rt_primes_3bytes_16align_alloc_layout_checked, 3, 16);
rt_alloc_layout_unchecked!(rt_primes_3bytes_16align_alloc_layout_unchecked, 3, 16);
rt_alloc_excess_unused!(rt_primes_3bytes_16align_alloc_excess_unused, 3, 16);
rt_alloc_excess_used!(rt_primes_3bytes_16align_alloc_excess_used, 3, 16);
rt_realloc_naive!(rt_primes_3bytes_16align_realloc_naive, 3, 16);
rt_realloc!(rt_primes_3bytes_16align_realloc, 3, 16);
rt_realloc_excess_unused!(rt_primes_3bytes_16align_realloc_excess_unused, 3, 16);
rt_realloc_excess_used!(rt_primes_3bytes_16align_realloc_excess_used, 3, 16);

rt_calloc!(rt_primes_7bytes_16align_calloc, 7, 16);
rt_mallocx!(rt_primes_7bytes_16align_mallocx, 7, 16);
rt_mallocx_zeroed!(rt_primes_7bytes_16align_mallocx_zeroed, 7, 16);
rt_mallocx_nallocx!(rt_primes_7bytes_16align_mallocx_nallocx, 7, 16);
rt_alloc_layout_checked!(rt_primes_7bytes_16align_alloc_layout_checked, 7, 16);
rt_alloc_layout_unchecked!(rt_primes_7bytes_16align_alloc_layout_unchecked, 7, 16);
rt_alloc_excess_unused!(rt_primes_7bytes_16align_alloc_excess_unused, 7, 16);
rt_alloc_excess_used!(rt_primes_7bytes_16align_alloc_excess_used, 7, 16);
rt_realloc_naive!(rt_primes_7bytes_16align_realloc_naive, 7, 16);
rt_realloc!(rt_primes_7bytes_16align_realloc, 7, 16);
rt_realloc_excess_unused!(rt_primes_7bytes_16align_realloc_excess_unused, 7, 16);
rt_realloc_excess_used!(rt_primes_7bytes_16align_realloc_excess_used, 7, 16);

rt_calloc!(rt_primes_13bytes_16align_calloc, 13, 16);
rt_mallocx!(rt_primes_13bytes_16align_mallocx, 13, 16);
rt_mallocx_zeroed!(rt_primes_13bytes_16align_mallocx_zeroed, 13, 16);
rt_mallocx_nallocx!(rt_primes_13bytes_16align_mallocx_nallocx, 13, 16);
rt_alloc_layout_checked!(rt_primes_13bytes_16align_alloc_layout_checked, 13, 16);
rt_alloc_layout_unchecked!(rt_primes_13bytes_16align_alloc_layout_unchecked, 13, 16);
rt_alloc_excess_unused!(rt_primes_13bytes_16align_alloc_excess_unused, 13, 16);
rt_alloc_excess_used!(rt_primes_13bytes_16align_alloc_excess_used, 13, 16);
rt_realloc_naive!(rt_primes_13bytes_16align_realloc_naive, 13, 16);
rt_realloc!(rt_primes_13bytes_16align_realloc, 13, 16);
rt_realloc_excess_unused!(rt_primes_13bytes_16align_realloc_excess_unused, 13, 16);
rt_realloc_excess_used!(rt_primes_13bytes_16align_realloc_excess_used, 13, 16);

rt_calloc!(rt_primes_17bytes_16align_calloc, 17, 16);
rt_mallocx!(rt_primes_17bytes_16align_mallocx, 17, 16);
rt_mallocx_zeroed!(rt_primes_17bytes_16align_mallocx_zeroed, 17, 16);
rt_mallocx_nallocx!(rt_primes_17bytes_16align_mallocx_nallocx, 17, 16);
rt_alloc_layout_checked!(rt_primes_17bytes_16align_alloc_layout_checked, 17, 16);
rt_alloc_layout_unchecked!(rt_primes_17bytes_16align_alloc_layout_unchecked, 17, 16);
rt_alloc_excess_unused!(rt_primes_17bytes_16align_alloc_excess_unused, 17, 16);
rt_alloc_excess_used!(rt_primes_17bytes_16align_alloc_excess_used, 17, 16);
rt_realloc_naive!(rt_primes_17bytes_16align_realloc_naive, 17, 16);
rt_realloc!(rt_primes_17bytes_16align_realloc, 17, 16);
rt_realloc_excess_unused!(rt_primes_17bytes_16align_realloc_excess_unused, 17, 16);
rt_realloc_excess_used!(rt_primes_17bytes_16align_realloc_excess_used, 17, 16);

rt_calloc!(rt_primes_31bytes_16align_calloc, 31, 16);
rt_mallocx!(rt_primes_31bytes_16align_mallocx, 31, 16);
rt_mallocx_zeroed!(rt_primes_31bytes_16align_mallocx_zeroed, 31, 16);
rt_mallocx_nallocx!(rt_primes_31bytes_16align_mallocx_nallocx, 31, 16);
rt_alloc_layout_checked!(rt_primes_31bytes_16align_alloc_layout_checked, 31, 16);
rt_alloc_layout_unchecked!(rt_primes_31bytes_16align_alloc_layout_unchecked, 31, 16);
rt_alloc_excess_unused!(rt_primes_31bytes_16align_alloc_excess_unused, 31, 16);
rt_alloc_excess_used!(rt_primes_31bytes_16align_alloc_excess_used, 31, 16);
rt_realloc_naive!(rt_primes_31bytes_16align_realloc_naive, 31, 16);
rt_realloc!(rt_primes_31bytes_16align_realloc, 31, 16);
rt_realloc_excess_unused!(rt_primes_31bytes_16align_realloc_excess_unused, 31, 16);
rt_realloc_excess_used!(rt_primes_31bytes_16align_realloc_excess_used, 31, 16);

rt_calloc!(rt_primes_61bytes_16align_calloc, 61, 16);
rt_mallocx!(rt_primes_61bytes_16align_mallocx, 61, 16);
rt_mallocx_zeroed!(rt_primes_61bytes_16align_mallocx_zeroed, 61, 16);
rt_mallocx_nallocx!(rt_primes_61bytes_16align_mallocx_nallocx, 61, 16);
rt_alloc_layout_checked!(rt_primes_61bytes_16align_alloc_layout_checked, 61, 16);
rt_alloc_layout_unchecked!(rt_primes_61bytes_16align_alloc_layout_unchecked, 61, 16);
rt_alloc_excess_unused!(rt_primes_61bytes_16align_alloc_excess_unused, 61, 16);
rt_alloc_excess_used!(rt_primes_61bytes_16align_alloc_excess_used, 61, 16);
rt_realloc_naive!(rt_primes_61bytes_16align_realloc_naive, 61, 16);
rt_realloc!(rt_primes_61bytes_16align_realloc, 61, 16);
rt_realloc_excess_unused!(rt_primes_61bytes_16align_realloc_excess_unused, 61, 16);
rt_realloc_excess_used!(rt_primes_61bytes_16align_realloc_excess_used, 61, 16);

rt_calloc!(rt_primes_96bytes_16align_calloc, 96, 16);
rt_mallocx!(rt_primes_96bytes_16align_mallocx, 96, 16);
rt_mallocx_zeroed!(rt_primes_96bytes_16align_mallocx_zeroed, 96, 16);
rt_mallocx_nallocx!(rt_primes_96bytes_16align_mallocx_nallocx, 96, 16);
rt_alloc_layout_checked!(rt_primes_96bytes_16align_alloc_layout_checked, 96, 16);
rt_alloc_layout_unchecked!(rt_primes_96bytes_16align_alloc_layout_unchecked, 96, 16);
rt_alloc_excess_unused!(rt_primes_96bytes_16align_alloc_excess_unused, 96, 16);
rt_alloc_excess_used!(rt_primes_96bytes_16align_alloc_excess_used, 96, 16);
rt_realloc_naive!(rt_primes_96bytes_16align_realloc_naive, 96, 16);
rt_realloc!(rt_primes_96bytes_16align_realloc, 96, 16);
rt_realloc_excess_unused!(rt_primes_96bytes_16align_realloc_excess_unused, 96, 16);
rt_realloc_excess_used!(rt_primes_96bytes_16align_realloc_excess_used, 96, 16);

rt_calloc!(rt_primes_127bytes_16align_calloc, 127, 16);
rt_mallocx!(rt_primes_127bytes_16align_mallocx, 127, 16);
rt_mallocx_zeroed!(rt_primes_127bytes_16align_mallocx_zeroed, 127, 16);
rt_mallocx_nallocx!(rt_primes_127bytes_16align_mallocx_nallocx, 127, 16);
rt_alloc_layout_checked!(rt_primes_127bytes_16align_alloc_layout_checked, 127, 16);
rt_alloc_layout_unchecked!(rt_primes_127bytes_16align_alloc_layout_unchecked, 127, 16);
rt_alloc_excess_unused!(rt_primes_127bytes_16align_alloc_excess_unused, 127, 16);
rt_alloc_excess_used!(rt_primes_127bytes_16align_alloc_excess_used, 127, 16);
rt_realloc_naive!(rt_primes_127bytes_16align_realloc_naive, 127, 16);
rt_realloc!(rt_primes_127bytes_16align_realloc, 127, 16);
rt_realloc_excess_unused!(rt_primes_127bytes_16align_realloc_excess_unused, 127, 16);
rt_realloc_excess_used!(rt_primes_127bytes_16align_realloc_excess_used, 127, 16);

rt_calloc!(rt_primes_257bytes_16align_calloc, 257, 16);
rt_mallocx!(rt_primes_257bytes_16align_mallocx, 257, 16);
rt_mallocx_zeroed!(rt_primes_257bytes_16align_mallocx_zeroed, 257, 16);
rt_mallocx_nallocx!(rt_primes_257bytes_16align_mallocx_nallocx, 257, 16);
rt_alloc_layout_checked!(rt_primes_257bytes_16align_alloc_layout_checked, 257, 16);
rt_alloc_layout_unchecked!(rt_primes_257bytes_16align_alloc_layout_unchecked, 257, 16);
rt_alloc_excess_unused!(rt_primes_257bytes_16align_alloc_excess_unused, 257, 16);
rt_alloc_excess_used!(rt_primes_257bytes_16align_alloc_excess_used, 257, 16);
rt_realloc_naive!(rt_primes_257bytes_16align_realloc_naive, 257, 16);
rt_realloc!(rt_primes_257bytes_16align_realloc, 257, 16);
rt_realloc_excess_unused!(rt_primes_257bytes_16align_realloc_excess_unused, 257, 16);
rt_realloc_excess_used!(rt_primes_257bytes_16align_realloc_excess_used, 257, 16);

rt_calloc!(rt_primes_509bytes_16align_calloc, 509, 16);
rt_mallocx!(rt_primes_509bytes_16align_mallocx, 509, 16);
rt_mallocx_zeroed!(rt_primes_509bytes_16align_mallocx_zeroed, 509, 16);
rt_mallocx_nallocx!(rt_primes_509bytes_16align_mallocx_nallocx, 509, 16);
rt_alloc_layout_checked!(rt_primes_509bytes_16align_alloc_layout_checked, 509, 16);
rt_alloc_layout_unchecked!(rt_primes_509bytes_16align_alloc_layout_unchecked, 509, 16);
rt_alloc_excess_unused!(rt_primes_509bytes_16align_alloc_excess_unused, 509, 16);
rt_alloc_excess_used!(rt_primes_509bytes_16align_alloc_excess_used, 509, 16);
rt_realloc_naive!(rt_primes_509bytes_16align_realloc_naive, 509, 16);
rt_realloc!(rt_primes_509bytes_16align_realloc, 509, 16);
rt_realloc_excess_unused!(rt_primes_509bytes_16align_realloc_excess_unused, 509, 16);
rt_realloc_excess_used!(rt_primes_509bytes_16align_realloc_excess_used, 509, 16);

rt_calloc!(rt_primes_1021bytes_16align_calloc, 1021, 16);
rt_mallocx!(rt_primes_1021bytes_16align_mallocx, 1021, 16);
rt_mallocx_zeroed!(rt_primes_1021bytes_16align_mallocx_zeroed, 1021, 16);
rt_mallocx_nallocx!(rt_primes_1021bytes_16align_mallocx_nallocx, 1021, 16);
rt_alloc_layout_checked!(rt_primes_1021bytes_16align_alloc_layout_checked, 1021, 16);
rt_alloc_layout_unchecked!(rt_primes_1021bytes_16align_alloc_layout_unchecked, 1021, 16);
rt_alloc_excess_unused!(rt_primes_1021bytes_16align_alloc_excess_unused, 1021, 16);
rt_alloc_excess_used!(rt_primes_1021bytes_16align_alloc_excess_used, 1021, 16);
rt_realloc_naive!(rt_primes_1021bytes_16align_realloc_naive, 1021, 16);
rt_realloc!(rt_primes_1021bytes_16align_realloc, 1021, 16);
rt_realloc_excess_unused!(rt_primes_1021bytes_16align_realloc_excess_unused, 1021, 16);
rt_realloc_excess_used!(rt_primes_1021bytes_16align_realloc_excess_used, 1021, 16);

rt_calloc!(rt_primes_2039bytes_16align_calloc, 2039, 16);
rt_mallocx!(rt_primes_2039bytes_16align_mallocx, 2039, 16);
rt_mallocx_zeroed!(rt_primes_2039bytes_16align_mallocx_zeroed, 2039, 16);
rt_mallocx_nallocx!(rt_primes_2039bytes_16align_mallocx_nallocx, 2039, 16);
rt_alloc_layout_checked!(rt_primes_2039bytes_16align_alloc_layout_checked, 2039, 16);
rt_alloc_layout_unchecked!(rt_primes_2039bytes_16align_alloc_layout_unchecked, 2039, 16);
rt_alloc_excess_unused!(rt_primes_2039bytes_16align_alloc_excess_unused, 2039, 16);
rt_alloc_excess_used!(rt_primes_2039bytes_16align_alloc_excess_used, 2039, 16);
rt_realloc_naive!(rt_primes_2039bytes_16align_realloc_naive, 2039, 16);
rt_realloc!(rt_primes_2039bytes_16align_realloc, 2039, 16);
rt_realloc_excess_unused!(rt_primes_2039bytes_16align_realloc_excess_unused, 2039, 16);
rt_realloc_excess_used!(rt_primes_2039bytes_16align_realloc_excess_used, 2039, 16);

rt_calloc!(rt_primes_4093bytes_16align_calloc, 4093, 16);
rt_mallocx!(rt_primes_4093bytes_16align_mallocx, 4093, 16);
rt_mallocx_zeroed!(rt_primes_4093bytes_16align_mallocx_zeroed, 4093, 16);
rt_mallocx_nallocx!(rt_primes_4093bytes_16align_mallocx_nallocx, 4093, 16);
rt_alloc_layout_checked!(rt_primes_4093bytes_16align_alloc_layout_checked, 4093, 16);
rt_alloc_layout_unchecked!(rt_primes_4093bytes_16align_alloc_layout_unchecked, 4093, 16);
rt_alloc_excess_unused!(rt_primes_4093bytes_16align_alloc_excess_unused, 4093, 16);
rt_alloc_excess_used!(rt_primes_4093bytes_16align_alloc_excess_used, 4093, 16);
rt_realloc_naive!(rt_primes_4093bytes_16align_realloc_naive, 4093, 16);
rt_realloc!(rt_primes_4093bytes_16align_realloc, 4093, 16);
rt_realloc_excess_unused!(rt_primes_4093bytes_16align_realloc_excess_unused, 4093, 16);
rt_realloc_excess_used!(rt_primes_4093bytes_16align_realloc_excess_used, 4093, 16);

rt_calloc!(rt_primes_8191bytes_16align_calloc, 8191, 16);
rt_mallocx!(rt_primes_8191bytes_16align_mallocx, 8191, 16);
rt_mallocx_zeroed!(rt_primes_8191bytes_16align_mallocx_zeroed, 8191, 16);
rt_mallocx_nallocx!(rt_primes_8191bytes_16align_mallocx_nallocx, 8191, 16);
rt_alloc_layout_checked!(rt_primes_8191bytes_16align_alloc_layout_checked, 8191, 16);
rt_alloc_layout_unchecked!(rt_primes_8191bytes_16align_alloc_layout_unchecked, 8191, 16);
rt_alloc_excess_unused!(rt_primes_8191bytes_16align_alloc_excess_unused, 8191, 16);
rt_alloc_excess_used!(rt_primes_8191bytes_16align_alloc_excess_used, 8191, 16);
rt_realloc_naive!(rt_primes_8191bytes_16align_realloc_naive, 8191, 16);
rt_realloc!(rt_primes_8191bytes_16align_realloc, 8191, 16);
rt_realloc_excess_unused!(rt_primes_8191bytes_16align_realloc_excess_unused, 8191, 16);
rt_realloc_excess_used!(rt_primes_8191bytes_16align_realloc_excess_used, 8191, 16);

rt_calloc!(rt_primes_16381bytes_16align_calloc, 16381, 16);
rt_mallocx!(rt_primes_16381bytes_16align_mallocx, 16381, 16);
rt_mallocx_zeroed!(rt_primes_16381bytes_16align_mallocx_zeroed, 16381, 16);
rt_mallocx_nallocx!(rt_primes_16381bytes_16align_mallocx_nallocx, 16381, 16);
rt_alloc_layout_checked!(rt_primes_16381bytes_16align_alloc_layout_checked, 16381, 16);
rt_alloc_layout_unchecked!(
    rt_primes_16381bytes_16align_alloc_layout_unchecked,
    16381,
    16
);
rt_alloc_excess_unused!(rt_primes_16381bytes_16align_alloc_excess_unused, 16381, 16);
rt_alloc_excess_used!(rt_primes_16381bytes_16align_alloc_excess_used, 16381, 16);
rt_realloc_naive!(rt_primes_16381bytes_16align_realloc_naive, 16381, 16);
rt_realloc!(rt_primes_16381bytes_16align_realloc, 16381, 16);
rt_realloc_excess_unused!(
    rt_primes_16381bytes_16align_realloc_excess_unused,
    16381,
    16
);
rt_realloc_excess_used!(rt_primes_16381bytes_16align_realloc_excess_used, 16381, 16);

rt_calloc!(rt_primes_32749bytes_16align_calloc, 32749, 16);
rt_mallocx!(rt_primes_32749bytes_16align_mallocx, 32749, 16);
rt_mallocx_zeroed!(rt_primes_32749bytes_16align_mallocx_zeroed, 32749, 16);
rt_mallocx_nallocx!(rt_primes_32749bytes_16align_mallocx_nallocx, 32749, 16);
rt_alloc_layout_checked!(rt_primes_32749bytes_16align_alloc_layout_checked, 32749, 16);
rt_alloc_layout_unchecked!(
    rt_primes_32749bytes_16align_alloc_layout_unchecked,
    32749,
    16
);
rt_alloc_excess_unused!(rt_primes_32749bytes_16align_alloc_excess_unused, 32749, 16);
rt_alloc_excess_used!(rt_primes_32749bytes_16align_alloc_excess_used, 32749, 16);
rt_realloc_naive!(rt_primes_32749bytes_16align_realloc_naive, 32749, 16);
rt_realloc!(rt_primes_32749bytes_16align_realloc, 32749, 16);
rt_realloc_excess_unused!(
    rt_primes_32749bytes_16align_realloc_excess_unused,
    32749,
    16
);
rt_realloc_excess_used!(rt_primes_32749bytes_16align_realloc_excess_used, 32749, 16);

rt_calloc!(rt_primes_65537bytes_16align_calloc, 65537, 16);
rt_mallocx!(rt_primes_65537bytes_16align_mallocx, 65537, 16);
rt_mallocx_zeroed!(rt_primes_65537bytes_16align_mallocx_zeroed, 65537, 16);
rt_mallocx_nallocx!(rt_primes_65537bytes_16align_mallocx_nallocx, 65537, 16);
rt_alloc_layout_checked!(rt_primes_65537bytes_16align_alloc_layout_checked, 65537, 16);
rt_alloc_layout_unchecked!(
    rt_primes_65537bytes_16align_alloc_layout_unchecked,
    65537,
    16
);
rt_alloc_excess_unused!(rt_primes_65537bytes_16align_alloc_excess_unused, 65537, 16);
rt_alloc_excess_used!(rt_primes_65537bytes_16align_alloc_excess_used, 65537, 16);
rt_realloc_naive!(rt_primes_65537bytes_16align_realloc_naive, 65537, 16);
rt_realloc!(rt_primes_65537bytes_16align_realloc, 65537, 16);
rt_realloc_excess_unused!(
    rt_primes_65537bytes_16align_realloc_excess_unused,
    65537,
    16
);
rt_realloc_excess_used!(rt_primes_65537bytes_16align_realloc_excess_used, 65537, 16);

rt_calloc!(rt_primes_131071bytes_16align_calloc, 131071, 16);
rt_mallocx!(rt_primes_131071bytes_16align_mallocx, 131071, 16);
rt_mallocx_zeroed!(rt_primes_131071bytes_16align_mallocx_zeroed, 131071, 16);
rt_mallocx_nallocx!(rt_primes_131071bytes_16align_mallocx_nallocx, 131071, 16);
rt_alloc_layout_checked!(
    rt_primes_131071bytes_16align_alloc_layout_checked,
    131071,
    16
);
rt_alloc_layout_unchecked!(
    rt_primes_131071bytes_16align_alloc_layout_unchecked,
    131071,
    16
);
rt_alloc_excess_unused!(
    rt_primes_131071bytes_16align_alloc_excess_unused,
    131071,
    16
);
rt_alloc_excess_used!(rt_primes_131071bytes_16align_alloc_excess_used, 131071, 16);
rt_realloc_naive!(rt_primes_131071bytes_16align_realloc_naive, 131071, 16);
rt_realloc!(rt_primes_131071bytes_16align_realloc, 131071, 16);
rt_realloc_excess_unused!(
    rt_primes_131071bytes_16align_realloc_excess_unused,
    131071,
    16
);
rt_realloc_excess_used!(
    rt_primes_131071bytes_16align_realloc_excess_used,
    131071,
    16
);

rt_calloc!(rt_primes_4194301bytes_16align_calloc, 4194301, 16);
rt_mallocx!(rt_primes_4194301bytes_16align_mallocx, 4194301, 16);
rt_mallocx_zeroed!(rt_primes_4194301bytes_16align_mallocx_zeroed, 4194301, 16);
rt_mallocx_nallocx!(rt_primes_4194301bytes_16align_mallocx_nallocx, 4194301, 16);
rt_alloc_layout_checked!(
    rt_primes_4194301bytes_16align_alloc_layout_checked,
    4194301,
    16
);
rt_alloc_layout_unchecked!(
    rt_primes_4194301bytes_16align_alloc_layout_unchecked,
    4194301,
    16
);
rt_alloc_excess_unused!(
    rt_primes_4194301bytes_16align_alloc_excess_unused,
    4194301,
    16
);
rt_alloc_excess_used!(
    rt_primes_4194301bytes_16align_alloc_excess_used,
    4194301,
    16
);
rt_realloc_naive!(rt_primes_4194301bytes_16align_realloc_naive, 4194301, 16);
rt_realloc!(rt_primes_4194301bytes_16align_realloc, 4194301, 16);
rt_realloc_excess_unused!(
    rt_primes_4194301bytes_16align_realloc_excess_unused,
    4194301,
    16
);
rt_realloc_excess_used!(
    rt_primes_4194301bytes_16align_realloc_excess_used,
    4194301,
    16
);

// 32 bytes alignment

// Powers of two:
rt_calloc!(rt_pow2_1bytes_32align_calloc, 1, 32);
rt_mallocx!(rt_pow2_1bytes_32align_mallocx, 1, 32);
rt_mallocx_zeroed!(rt_pow2_1bytes_32align_mallocx_zeroed, 1, 32);
rt_mallocx_nallocx!(rt_pow2_1bytes_32align_mallocx_nallocx, 1, 32);
rt_alloc_layout_checked!(rt_pow2_1bytes_32align_alloc_layout_checked, 1, 32);
rt_alloc_layout_unchecked!(rt_pow2_1bytes_32align_alloc_layout_unchecked, 1, 32);
rt_alloc_excess_unused!(rt_pow2_1bytes_32align_alloc_excess_unused, 1, 32);
rt_alloc_excess_used!(rt_pow2_1bytes_32align_alloc_excess_used, 1, 32);
rt_realloc_naive!(rt_pow2_1bytes_32align_realloc_naive, 1, 32);
rt_realloc!(rt_pow2_1bytes_32align_realloc, 1, 32);
rt_realloc_excess_unused!(rt_pow2_1bytes_32align_realloc_excess_unused, 1, 32);
rt_realloc_excess_used!(rt_pow2_1bytes_32align_realloc_excess_used, 1, 32);

rt_calloc!(rt_pow2_2bytes_32align_calloc, 2, 32);
rt_mallocx!(rt_pow2_2bytes_32align_mallocx, 2, 32);
rt_mallocx_zeroed!(rt_pow2_2bytes_32align_mallocx_zeroed, 2, 32);
rt_mallocx_nallocx!(rt_pow2_2bytes_32align_mallocx_nallocx, 2, 32);
rt_alloc_layout_checked!(rt_pow2_2bytes_32align_alloc_layout_checked, 2, 32);
rt_alloc_layout_unchecked!(rt_pow2_2bytes_32align_alloc_layout_unchecked, 2, 32);
rt_alloc_excess_unused!(rt_pow2_2bytes_32align_alloc_excess_unused, 2, 32);
rt_alloc_excess_used!(rt_pow2_2bytes_32align_alloc_excess_used, 2, 32);
rt_realloc_naive!(rt_pow2_2bytes_32align_realloc_naive, 2, 32);
rt_realloc!(rt_pow2_2bytes_32align_realloc, 2, 32);
rt_realloc_excess_unused!(rt_pow2_2bytes_32align_realloc_excess_unused, 2, 32);
rt_realloc_excess_used!(rt_pow2_2bytes_32align_realloc_excess_used, 2, 32);

rt_calloc!(rt_pow2_4bytes_32align_calloc, 4, 32);
rt_mallocx!(rt_pow2_4bytes_32align_mallocx, 4, 32);
rt_mallocx_zeroed!(rt_pow2_4bytes_32align_mallocx_zeroed, 4, 32);
rt_mallocx_nallocx!(rt_pow2_4bytes_32align_mallocx_nallocx, 4, 32);
rt_alloc_layout_checked!(rt_pow2_4bytes_32align_alloc_layout_checked, 4, 32);
rt_alloc_layout_unchecked!(rt_pow2_4bytes_32align_alloc_layout_unchecked, 4, 32);
rt_alloc_excess_unused!(rt_pow2_4bytes_32align_alloc_excess_unused, 4, 32);
rt_alloc_excess_used!(rt_pow2_4bytes_32align_alloc_excess_used, 4, 32);
rt_realloc_naive!(rt_pow2_4bytes_32align_realloc_naive, 4, 32);
rt_realloc!(rt_pow2_4bytes_32align_realloc, 4, 32);
rt_realloc_excess_unused!(rt_pow2_4bytes_32align_realloc_excess_unused, 4, 32);
rt_realloc_excess_used!(rt_pow2_4bytes_32align_realloc_excess_used, 4, 32);

rt_calloc!(rt_pow2_8bytes_32align_calloc, 8, 32);
rt_mallocx!(rt_pow2_8bytes_32align_mallocx, 8, 32);
rt_mallocx_zeroed!(rt_pow2_8bytes_32align_mallocx_zeroed, 8, 32);
rt_mallocx_nallocx!(rt_pow2_8bytes_32align_mallocx_nallocx, 8, 32);
rt_alloc_layout_checked!(rt_pow2_8bytes_32align_alloc_layout_checked, 8, 32);
rt_alloc_layout_unchecked!(rt_pow2_8bytes_32align_alloc_layout_unchecked, 8, 32);
rt_alloc_excess_unused!(rt_pow2_8bytes_32align_alloc_excess_unused, 8, 32);
rt_alloc_excess_used!(rt_pow2_8bytes_32align_alloc_excess_used, 8, 32);
rt_realloc_naive!(rt_pow2_8bytes_32align_realloc_naive, 8, 32);
rt_realloc!(rt_pow2_8bytes_32align_realloc, 8, 32);
rt_realloc_excess_unused!(rt_pow2_8bytes_32align_realloc_excess_unused, 8, 32);
rt_realloc_excess_used!(rt_pow2_8bytes_32align_realloc_excess_used, 8, 32);

rt_calloc!(rt_pow2_16bytes_32align_calloc, 16, 32);
rt_mallocx!(rt_pow2_16bytes_32align_mallocx, 16, 32);
rt_mallocx_zeroed!(rt_pow2_16bytes_32align_mallocx_zeroed, 16, 32);
rt_mallocx_nallocx!(rt_pow2_16bytes_32align_mallocx_nallocx, 16, 32);
rt_alloc_layout_checked!(rt_pow2_16bytes_32align_alloc_layout_checked, 16, 32);
rt_alloc_layout_unchecked!(rt_pow2_16bytes_32align_alloc_layout_unchecked, 16, 32);
rt_alloc_excess_unused!(rt_pow2_16bytes_32align_alloc_excess_unused, 16, 32);
rt_alloc_excess_used!(rt_pow2_16bytes_32align_alloc_excess_used, 16, 32);
rt_realloc_naive!(rt_pow2_16bytes_32align_realloc_naive, 16, 32);
rt_realloc!(rt_pow2_16bytes_32align_realloc, 16, 32);
rt_realloc_excess_unused!(rt_pow2_16bytes_32align_realloc_excess_unused, 16, 32);
rt_realloc_excess_used!(rt_pow2_16bytes_32align_realloc_excess_used, 16, 32);

rt_calloc!(rt_pow2_32bytes_32align_calloc, 32, 32);
rt_mallocx!(rt_pow2_32bytes_32align_mallocx, 32, 32);
rt_mallocx_zeroed!(rt_pow2_32bytes_32align_mallocx_zeroed, 32, 32);
rt_mallocx_nallocx!(rt_pow2_32bytes_32align_mallocx_nallocx, 32, 32);
rt_alloc_layout_checked!(rt_pow2_32bytes_32align_alloc_layout_checked, 32, 32);
rt_alloc_layout_unchecked!(rt_pow2_32bytes_32align_alloc_layout_unchecked, 32, 32);
rt_alloc_excess_unused!(rt_pow2_32bytes_32align_alloc_excess_unused, 32, 32);
rt_alloc_excess_used!(rt_pow2_32bytes_32align_alloc_excess_used, 32, 32);
rt_realloc_naive!(rt_pow2_32bytes_32align_realloc_naive, 32, 32);
rt_realloc!(rt_pow2_32bytes_32align_realloc, 32, 32);
rt_realloc_excess_unused!(rt_pow2_32bytes_32align_realloc_excess_unused, 32, 32);
rt_realloc_excess_used!(rt_pow2_32bytes_32align_realloc_excess_used, 32, 32);

rt_calloc!(rt_pow2_64bytes_32align_calloc, 64, 32);
rt_mallocx!(rt_pow2_64bytes_32align_mallocx, 64, 32);
rt_mallocx_zeroed!(rt_pow2_64bytes_32align_mallocx_zeroed, 64, 32);
rt_mallocx_nallocx!(rt_pow2_64bytes_32align_mallocx_nallocx, 64, 32);
rt_alloc_layout_checked!(rt_pow2_64bytes_32align_alloc_layout_checked, 64, 32);
rt_alloc_layout_unchecked!(rt_pow2_64bytes_32align_alloc_layout_unchecked, 64, 32);
rt_alloc_excess_unused!(rt_pow2_64bytes_32align_alloc_excess_unused, 64, 32);
rt_alloc_excess_used!(rt_pow2_64bytes_32align_alloc_excess_used, 64, 32);
rt_realloc_naive!(rt_pow2_64bytes_32align_realloc_naive, 64, 32);
rt_realloc!(rt_pow2_64bytes_32align_realloc, 64, 32);
rt_realloc_excess_unused!(rt_pow2_64bytes_32align_realloc_excess_unused, 64, 32);
rt_realloc_excess_used!(rt_pow2_64bytes_32align_realloc_excess_used, 64, 32);

rt_calloc!(rt_pow2_128bytes_32align_calloc, 128, 32);
rt_mallocx!(rt_pow2_128bytes_32align_mallocx, 128, 32);
rt_mallocx_zeroed!(rt_pow2_128bytes_32align_mallocx_zeroed, 128, 32);
rt_mallocx_nallocx!(rt_pow2_128bytes_32align_mallocx_nallocx, 128, 32);
rt_alloc_layout_checked!(rt_pow2_128bytes_32align_alloc_layout_checked, 128, 32);
rt_alloc_layout_unchecked!(rt_pow2_128bytes_32align_alloc_layout_unchecked, 128, 32);
rt_alloc_excess_unused!(rt_pow2_128bytes_32align_alloc_excess_unused, 128, 32);
rt_alloc_excess_used!(rt_pow2_128bytes_32align_alloc_excess_used, 128, 32);
rt_realloc_naive!(rt_pow2_128bytes_32align_realloc_naive, 128, 32);
rt_realloc!(rt_pow2_128bytes_32align_realloc, 128, 32);
rt_realloc_excess_unused!(rt_pow2_128bytes_32align_realloc_excess_unused, 128, 32);
rt_realloc_excess_used!(rt_pow2_128bytes_32align_realloc_excess_used, 128, 32);

rt_calloc!(rt_pow2_256bytes_32align_calloc, 256, 32);
rt_mallocx!(rt_pow2_256bytes_32align_mallocx, 256, 32);
rt_mallocx_zeroed!(rt_pow2_256bytes_32align_mallocx_zeroed, 256, 32);
rt_mallocx_nallocx!(rt_pow2_256bytes_32align_mallocx_nallocx, 256, 32);
rt_alloc_layout_checked!(rt_pow2_256bytes_32align_alloc_layout_checked, 256, 32);
rt_alloc_layout_unchecked!(rt_pow2_256bytes_32align_alloc_layout_unchecked, 256, 32);
rt_alloc_excess_unused!(rt_pow2_256bytes_32align_alloc_excess_unused, 256, 32);
rt_alloc_excess_used!(rt_pow2_256bytes_32align_alloc_excess_used, 256, 32);
rt_realloc_naive!(rt_pow2_256bytes_32align_realloc_naive, 256, 32);
rt_realloc!(rt_pow2_256bytes_32align_realloc, 256, 32);
rt_realloc_excess_unused!(rt_pow2_256bytes_32align_realloc_excess_unused, 256, 32);
rt_realloc_excess_used!(rt_pow2_256bytes_32align_realloc_excess_used, 256, 32);

rt_calloc!(rt_pow2_512bytes_32align_calloc, 512, 32);
rt_mallocx!(rt_pow2_512bytes_32align_mallocx, 512, 32);
rt_mallocx_zeroed!(rt_pow2_512bytes_32align_mallocx_zeroed, 512, 32);
rt_mallocx_nallocx!(rt_pow2_512bytes_32align_mallocx_nallocx, 512, 32);
rt_alloc_layout_checked!(rt_pow2_512bytes_32align_alloc_layout_checked, 512, 32);
rt_alloc_layout_unchecked!(rt_pow2_512bytes_32align_alloc_layout_unchecked, 512, 32);
rt_alloc_excess_unused!(rt_pow2_512bytes_32align_alloc_excess_unused, 512, 32);
rt_alloc_excess_used!(rt_pow2_512bytes_32align_alloc_excess_used, 512, 32);
rt_realloc_naive!(rt_pow2_512bytes_32align_realloc_naive, 512, 32);
rt_realloc!(rt_pow2_512bytes_32align_realloc, 512, 32);
rt_realloc_excess_unused!(rt_pow2_512bytes_32align_realloc_excess_unused, 512, 32);
rt_realloc_excess_used!(rt_pow2_512bytes_32align_realloc_excess_used, 512, 32);

rt_calloc!(rt_pow2_1024bytes_32align_calloc, 1024, 32);
rt_mallocx!(rt_pow2_1024bytes_32align_mallocx, 1024, 32);
rt_mallocx_zeroed!(rt_pow2_1024bytes_32align_mallocx_zeroed, 1024, 32);
rt_mallocx_nallocx!(rt_pow2_1024bytes_32align_mallocx_nallocx, 1024, 32);
rt_alloc_layout_checked!(rt_pow2_1024bytes_32align_alloc_layout_checked, 1024, 32);
rt_alloc_layout_unchecked!(rt_pow2_1024bytes_32align_alloc_layout_unchecked, 1024, 32);
rt_alloc_excess_unused!(rt_pow2_1024bytes_32align_alloc_excess_unused, 1024, 32);
rt_alloc_excess_used!(rt_pow2_1024bytes_32align_alloc_excess_used, 1024, 32);
rt_realloc_naive!(rt_pow2_1024bytes_32align_realloc_naive, 1024, 32);
rt_realloc!(rt_pow2_1024bytes_32align_realloc, 1024, 32);
rt_realloc_excess_unused!(rt_pow2_1024bytes_32align_realloc_excess_unused, 1024, 32);
rt_realloc_excess_used!(rt_pow2_1024bytes_32align_realloc_excess_used, 1024, 32);

rt_calloc!(rt_pow2_2048bytes_32align_calloc, 2048, 32);
rt_mallocx!(rt_pow2_2048bytes_32align_mallocx, 2048, 32);
rt_mallocx_zeroed!(rt_pow2_2048bytes_32align_mallocx_zeroed, 2048, 32);
rt_mallocx_nallocx!(rt_pow2_2048bytes_32align_mallocx_nallocx, 2048, 32);
rt_alloc_layout_checked!(rt_pow2_2048bytes_32align_alloc_layout_checked, 2048, 32);
rt_alloc_layout_unchecked!(rt_pow2_2048bytes_32align_alloc_layout_unchecked, 2048, 32);
rt_alloc_excess_unused!(rt_pow2_2048bytes_32align_alloc_excess_unused, 2048, 32);
rt_alloc_excess_used!(rt_pow2_2048bytes_32align_alloc_excess_used, 2048, 32);
rt_realloc_naive!(rt_pow2_2048bytes_32align_realloc_naive, 2048, 32);
rt_realloc!(rt_pow2_2048bytes_32align_realloc, 2048, 32);
rt_realloc_excess_unused!(rt_pow2_2048bytes_32align_realloc_excess_unused, 2048, 32);
rt_realloc_excess_used!(rt_pow2_2048bytes_32align_realloc_excess_used, 2048, 32);

rt_calloc!(rt_pow2_4096bytes_32align_calloc, 4096, 32);
rt_mallocx!(rt_pow2_4096bytes_32align_mallocx, 4096, 32);
rt_mallocx_zeroed!(rt_pow2_4096bytes_32align_mallocx_zeroed, 4096, 32);
rt_mallocx_nallocx!(rt_pow2_4096bytes_32align_mallocx_nallocx, 4096, 32);
rt_alloc_layout_checked!(rt_pow2_4096bytes_32align_alloc_layout_checked, 4096, 32);
rt_alloc_layout_unchecked!(rt_pow2_4096bytes_32align_alloc_layout_unchecked, 4096, 32);
rt_alloc_excess_unused!(rt_pow2_4096bytes_32align_alloc_excess_unused, 4096, 32);
rt_alloc_excess_used!(rt_pow2_4096bytes_32align_alloc_excess_used, 4096, 32);
rt_realloc_naive!(rt_pow2_4096bytes_32align_realloc_naive, 4096, 32);
rt_realloc!(rt_pow2_4096bytes_32align_realloc, 4096, 32);
rt_realloc_excess_unused!(rt_pow2_4096bytes_32align_realloc_excess_unused, 4096, 32);
rt_realloc_excess_used!(rt_pow2_4096bytes_32align_realloc_excess_used, 4096, 32);

rt_calloc!(rt_pow2_8192bytes_32align_calloc, 8192, 32);
rt_mallocx!(rt_pow2_8192bytes_32align_mallocx, 8192, 32);
rt_mallocx_zeroed!(rt_pow2_8192bytes_32align_mallocx_zeroed, 8192, 32);
rt_mallocx_nallocx!(rt_pow2_8192bytes_32align_mallocx_nallocx, 8192, 32);
rt_alloc_layout_checked!(rt_pow2_8192bytes_32align_alloc_layout_checked, 8192, 32);
rt_alloc_layout_unchecked!(rt_pow2_8192bytes_32align_alloc_layout_unchecked, 8192, 32);
rt_alloc_excess_unused!(rt_pow2_8192bytes_32align_alloc_excess_unused, 8192, 32);
rt_alloc_excess_used!(rt_pow2_8192bytes_32align_alloc_excess_used, 8192, 32);
rt_realloc_naive!(rt_pow2_8192bytes_32align_realloc_naive, 8192, 32);
rt_realloc!(rt_pow2_8192bytes_32align_realloc, 8192, 32);
rt_realloc_excess_unused!(rt_pow2_8192bytes_32align_realloc_excess_unused, 8192, 32);
rt_realloc_excess_used!(rt_pow2_8192bytes_32align_realloc_excess_used, 8192, 32);

rt_calloc!(rt_pow2_16384bytes_32align_calloc, 16384, 32);
rt_mallocx!(rt_pow2_16384bytes_32align_mallocx, 16384, 32);
rt_mallocx_zeroed!(rt_pow2_16384bytes_32align_mallocx_zeroed, 16384, 32);
rt_mallocx_nallocx!(rt_pow2_16384bytes_32align_mallocx_nallocx, 16384, 32);
rt_alloc_layout_checked!(rt_pow2_16384bytes_32align_alloc_layout_checked, 16384, 32);
rt_alloc_layout_unchecked!(rt_pow2_16384bytes_32align_alloc_layout_unchecked, 16384, 32);
rt_alloc_excess_unused!(rt_pow2_16384bytes_32align_alloc_excess_unused, 16384, 32);
rt_alloc_excess_used!(rt_pow2_16384bytes_32align_alloc_excess_used, 16384, 32);
rt_realloc_naive!(rt_pow2_16384bytes_32align_realloc_naive, 16384, 32);
rt_realloc!(rt_pow2_16384bytes_32align_realloc, 16384, 32);
rt_realloc_excess_unused!(rt_pow2_16384bytes_32align_realloc_excess_unused, 16384, 32);
rt_realloc_excess_used!(rt_pow2_16384bytes_32align_realloc_excess_used, 16384, 32);

rt_calloc!(rt_pow2_32768bytes_32align_calloc, 32768, 32);
rt_mallocx!(rt_pow2_32768bytes_32align_mallocx, 32768, 32);
rt_mallocx_zeroed!(rt_pow2_32768bytes_32align_mallocx_zeroed, 32768, 32);
rt_mallocx_nallocx!(rt_pow2_32768bytes_32align_mallocx_nallocx, 32768, 32);
rt_alloc_layout_checked!(rt_pow2_32768bytes_32align_alloc_layout_checked, 32768, 32);
rt_alloc_layout_unchecked!(rt_pow2_32768bytes_32align_alloc_layout_unchecked, 32768, 32);
rt_alloc_excess_unused!(rt_pow2_32768bytes_32align_alloc_excess_unused, 32768, 32);
rt_alloc_excess_used!(rt_pow2_32768bytes_32align_alloc_excess_used, 32768, 32);
rt_realloc_naive!(rt_pow2_32768bytes_32align_realloc_naive, 32768, 32);
rt_realloc!(rt_pow2_32768bytes_32align_realloc, 32768, 32);
rt_realloc_excess_unused!(rt_pow2_32768bytes_32align_realloc_excess_unused, 32768, 32);
rt_realloc_excess_used!(rt_pow2_32768bytes_32align_realloc_excess_used, 32768, 32);

rt_calloc!(rt_pow2_65536bytes_32align_calloc, 65536, 32);
rt_mallocx!(rt_pow2_65536bytes_32align_mallocx, 65536, 32);
rt_mallocx_zeroed!(rt_pow2_65536bytes_32align_mallocx_zeroed, 65536, 32);
rt_mallocx_nallocx!(rt_pow2_65536bytes_32align_mallocx_nallocx, 65536, 32);
rt_alloc_layout_checked!(rt_pow2_65536bytes_32align_alloc_layout_checked, 65536, 32);
rt_alloc_layout_unchecked!(rt_pow2_65536bytes_32align_alloc_layout_unchecked, 65536, 32);
rt_alloc_excess_unused!(rt_pow2_65536bytes_32align_alloc_excess_unused, 65536, 32);
rt_alloc_excess_used!(rt_pow2_65536bytes_32align_alloc_excess_used, 65536, 32);
rt_realloc_naive!(rt_pow2_65536bytes_32align_realloc_naive, 65536, 32);
rt_realloc!(rt_pow2_65536bytes_32align_realloc, 65536, 32);
rt_realloc_excess_unused!(rt_pow2_65536bytes_32align_realloc_excess_unused, 65536, 32);
rt_realloc_excess_used!(rt_pow2_65536bytes_32align_realloc_excess_used, 65536, 32);

rt_calloc!(rt_pow2_131072bytes_32align_calloc, 131072, 32);
rt_mallocx!(rt_pow2_131072bytes_32align_mallocx, 131072, 32);
rt_mallocx_zeroed!(rt_pow2_131072bytes_32align_mallocx_zeroed, 131072, 32);
rt_mallocx_nallocx!(rt_pow2_131072bytes_32align_mallocx_nallocx, 131072, 32);
rt_alloc_layout_checked!(rt_pow2_131072bytes_32align_alloc_layout_checked, 131072, 32);
rt_alloc_layout_unchecked!(
    rt_pow2_131072bytes_32align_alloc_layout_unchecked,
    131072,
    32
);
rt_alloc_excess_unused!(rt_pow2_131072bytes_32align_alloc_excess_unused, 131072, 32);
rt_alloc_excess_used!(rt_pow2_131072bytes_32align_alloc_excess_used, 131072, 32);
rt_realloc_naive!(rt_pow2_131072bytes_32align_realloc_naive, 131072, 32);
rt_realloc!(rt_pow2_131072bytes_32align_realloc, 131072, 32);
rt_realloc_excess_unused!(
    rt_pow2_131072bytes_32align_realloc_excess_unused,
    131072,
    32
);
rt_realloc_excess_used!(rt_pow2_131072bytes_32align_realloc_excess_used, 131072, 32);

rt_calloc!(rt_pow2_4194304bytes_32align_calloc, 4194304, 32);
rt_mallocx!(rt_pow2_4194304bytes_32align_mallocx, 4194304, 32);
rt_mallocx_zeroed!(rt_pow2_4194304bytes_32align_mallocx_zeroed, 4194304, 32);
rt_mallocx_nallocx!(rt_pow2_4194304bytes_32align_mallocx_nallocx, 4194304, 32);
rt_alloc_layout_checked!(
    rt_pow2_4194304bytes_32align_alloc_layout_checked,
    4194304,
    32
);
rt_alloc_layout_unchecked!(
    rt_pow2_4194304bytes_32align_alloc_layout_unchecked,
    4194304,
    32
);
rt_alloc_excess_unused!(
    rt_pow2_4194304bytes_32align_alloc_excess_unused,
    4194304,
    32
);
rt_alloc_excess_used!(rt_pow2_4194304bytes_32align_alloc_excess_used, 4194304, 32);
rt_realloc_naive!(rt_pow2_4194304bytes_32align_realloc_naive, 4194304, 32);
rt_realloc!(rt_pow2_4194304bytes_32align_realloc, 4194304, 32);
rt_realloc_excess_unused!(
    rt_pow2_4194304bytes_32align_realloc_excess_unused,
    4194304,
    32
);
rt_realloc_excess_used!(
    rt_pow2_4194304bytes_32align_realloc_excess_used,
    4194304,
    32
);

// Even
rt_calloc!(rt_even_10bytes_32align_calloc, 10, 32);
rt_mallocx!(rt_even_10bytes_32align_mallocx, 10, 32);
rt_mallocx_zeroed!(rt_even_10bytes_32align_mallocx_zeroed, 10, 32);
rt_mallocx_nallocx!(rt_even_10bytes_32align_mallocx_nallocx, 10, 32);
rt_alloc_layout_checked!(rt_even_10bytes_32align_alloc_layout_checked, 10, 32);
rt_alloc_layout_unchecked!(rt_even_10bytes_32align_alloc_layout_unchecked, 10, 32);
rt_alloc_excess_unused!(rt_even_10bytes_32align_alloc_excess_unused, 10, 32);
rt_alloc_excess_used!(rt_even_10bytes_32align_alloc_excess_used, 10, 32);
rt_realloc_naive!(rt_even_10bytes_32align_realloc_naive, 10, 32);
rt_realloc!(rt_even_10bytes_32align_realloc, 10, 32);
rt_realloc_excess_unused!(rt_even_10bytes_32align_realloc_excess_unused, 10, 32);
rt_realloc_excess_used!(rt_even_10bytes_32align_realloc_excess_used, 10, 32);

rt_calloc!(rt_even_100bytes_32align_calloc, 100, 32);
rt_mallocx!(rt_even_100bytes_32align_mallocx, 100, 32);
rt_mallocx_zeroed!(rt_even_100bytes_32align_mallocx_zeroed, 100, 32);
rt_mallocx_nallocx!(rt_even_100bytes_32align_mallocx_nallocx, 100, 32);
rt_alloc_layout_checked!(rt_even_100bytes_32align_alloc_layout_checked, 100, 32);
rt_alloc_layout_unchecked!(rt_even_100bytes_32align_alloc_layout_unchecked, 100, 32);
rt_alloc_excess_unused!(rt_even_100bytes_32align_alloc_excess_unused, 100, 32);
rt_alloc_excess_used!(rt_even_100bytes_32align_alloc_excess_used, 100, 32);
rt_realloc_naive!(rt_even_100bytes_32align_realloc_naive, 100, 32);
rt_realloc!(rt_even_100bytes_32align_realloc, 100, 32);
rt_realloc_excess_unused!(rt_even_100bytes_32align_realloc_excess_unused, 100, 32);
rt_realloc_excess_used!(rt_even_100bytes_32align_realloc_excess_used, 100, 32);

rt_calloc!(rt_even_1000bytes_32align_calloc, 1000, 32);
rt_mallocx!(rt_even_1000bytes_32align_mallocx, 1000, 32);
rt_mallocx_zeroed!(rt_even_1000bytes_32align_mallocx_zeroed, 1000, 32);
rt_mallocx_nallocx!(rt_even_1000bytes_32align_mallocx_nallocx, 1000, 32);
rt_alloc_layout_checked!(rt_even_1000bytes_32align_alloc_layout_checked, 1000, 32);
rt_alloc_layout_unchecked!(rt_even_1000bytes_32align_alloc_layout_unchecked, 1000, 32);
rt_alloc_excess_unused!(rt_even_1000bytes_32align_alloc_excess_unused, 1000, 32);
rt_alloc_excess_used!(rt_even_1000bytes_32align_alloc_excess_used, 1000, 32);
rt_realloc_naive!(rt_even_1000bytes_32align_realloc_naive, 1000, 32);
rt_realloc!(rt_even_1000bytes_32align_realloc, 1000, 32);
rt_realloc_excess_unused!(rt_even_1000bytes_32align_realloc_excess_unused, 1000, 32);
rt_realloc_excess_used!(rt_even_1000bytes_32align_realloc_excess_used, 1000, 32);

rt_calloc!(rt_even_10000bytes_32align_calloc, 10000, 32);
rt_mallocx!(rt_even_10000bytes_32align_mallocx, 10000, 32);
rt_mallocx_zeroed!(rt_even_10000bytes_32align_mallocx_zeroed, 10000, 32);
rt_mallocx_nallocx!(rt_even_10000bytes_32align_mallocx_nallocx, 10000, 32);
rt_alloc_layout_checked!(rt_even_10000bytes_32align_alloc_layout_checked, 10000, 32);
rt_alloc_layout_unchecked!(rt_even_10000bytes_32align_alloc_layout_unchecked, 10000, 32);
rt_alloc_excess_unused!(rt_even_10000bytes_32align_alloc_excess_unused, 10000, 32);
rt_alloc_excess_used!(rt_even_10000bytes_32align_alloc_excess_used, 10000, 32);
rt_realloc_naive!(rt_even_10000bytes_32align_realloc_naive, 10000, 32);
rt_realloc!(rt_even_10000bytes_32align_realloc, 10000, 32);
rt_realloc_excess_unused!(rt_even_10000bytes_32align_realloc_excess_unused, 10000, 32);
rt_realloc_excess_used!(rt_even_10000bytes_32align_realloc_excess_used, 10000, 32);

rt_calloc!(rt_even_100000bytes_32align_calloc, 100000, 32);
rt_mallocx!(rt_even_100000bytes_32align_mallocx, 100000, 32);
rt_mallocx_zeroed!(rt_even_100000bytes_32align_mallocx_zeroed, 100000, 32);
rt_mallocx_nallocx!(rt_even_100000bytes_32align_mallocx_nallocx, 100000, 32);
rt_alloc_layout_checked!(rt_even_100000bytes_32align_alloc_layout_checked, 100000, 32);
rt_alloc_layout_unchecked!(
    rt_even_100000bytes_32align_alloc_layout_unchecked,
    100000,
    32
);
rt_alloc_excess_unused!(rt_even_100000bytes_32align_alloc_excess_unused, 100000, 32);
rt_alloc_excess_used!(rt_even_100000bytes_32align_alloc_excess_used, 100000, 32);
rt_realloc_naive!(rt_even_100000bytes_32align_realloc_naive, 100000, 32);
rt_realloc!(rt_even_100000bytes_32align_realloc, 100000, 32);
rt_realloc_excess_unused!(
    rt_even_100000bytes_32align_realloc_excess_unused,
    100000,
    32
);
rt_realloc_excess_used!(rt_even_100000bytes_32align_realloc_excess_used, 100000, 32);

rt_calloc!(rt_even_1000000bytes_32align_calloc, 1000000, 32);
rt_mallocx!(rt_even_1000000bytes_32align_mallocx, 1000000, 32);
rt_mallocx_zeroed!(rt_even_1000000bytes_32align_mallocx_zeroed, 1000000, 32);
rt_mallocx_nallocx!(rt_even_1000000bytes_32align_mallocx_nallocx, 1000000, 32);
rt_alloc_layout_checked!(
    rt_even_1000000bytes_32align_alloc_layout_checked,
    1000000,
    32
);
rt_alloc_layout_unchecked!(
    rt_even_1000000bytes_32align_alloc_layout_unchecked,
    1000000,
    32
);
rt_alloc_excess_unused!(
    rt_even_1000000bytes_32align_alloc_excess_unused,
    1000000,
    32
);
rt_alloc_excess_used!(rt_even_1000000bytes_32align_alloc_excess_used, 1000000, 32);
rt_realloc_naive!(rt_even_1000000bytes_32align_realloc_naive, 1000000, 32);
rt_realloc!(rt_even_1000000bytes_32align_realloc, 1000000, 32);
rt_realloc_excess_unused!(
    rt_even_1000000bytes_32align_realloc_excess_unused,
    1000000,
    32
);
rt_realloc_excess_used!(
    rt_even_1000000bytes_32align_realloc_excess_used,
    1000000,
    32
);

// Odd:
rt_calloc!(rt_odd_10bytes_32align_calloc, 10 - 1, 32);
rt_mallocx!(rt_odd_10bytes_32align_mallocx, 10 - 1, 32);
rt_mallocx_zeroed!(rt_odd_10bytes_32align_mallocx_zeroed, 10 - 1, 32);
rt_mallocx_nallocx!(rt_odd_10bytes_32align_mallocx_nallocx, 10 - 1, 32);
rt_alloc_layout_checked!(rt_odd_10bytes_32align_alloc_layout_checked, 10 - 1, 32);
rt_alloc_layout_unchecked!(rt_odd_10bytes_32align_alloc_layout_unchecked, 10 - 1, 32);
rt_alloc_excess_unused!(rt_odd_10bytes_32align_alloc_excess_unused, 10 - 1, 32);
rt_alloc_excess_used!(rt_odd_10bytes_32align_alloc_excess_used, 10 - 1, 32);
rt_realloc_naive!(rt_odd_10bytes_32align_realloc_naive, 10 - 1, 32);
rt_realloc!(rt_odd_10bytes_32align_realloc, 10 - 1, 32);
rt_realloc_excess_unused!(rt_odd_10bytes_32align_realloc_excess_unused, 10 - 1, 32);
rt_realloc_excess_used!(rt_odd_10bytes_32align_realloc_excess_used, 10 - 1, 32);

rt_calloc!(rt_odd_100bytes_32align_calloc, 100 - 1, 32);
rt_mallocx!(rt_odd_100bytes_32align_mallocx, 100 - 1, 32);
rt_mallocx_zeroed!(rt_odd_100bytes_32align_mallocx_zeroed, 100 - 1, 32);
rt_mallocx_nallocx!(rt_odd_100bytes_32align_mallocx_nallocx, 100 - 1, 32);
rt_alloc_layout_checked!(rt_odd_100bytes_32align_alloc_layout_checked, 100 - 1, 32);
rt_alloc_layout_unchecked!(rt_odd_100bytes_32align_alloc_layout_unchecked, 100 - 1, 32);
rt_alloc_excess_unused!(rt_odd_100bytes_32align_alloc_excess_unused, 100 - 1, 32);
rt_alloc_excess_used!(rt_odd_100bytes_32align_alloc_excess_used, 100 - 1, 32);
rt_realloc_naive!(rt_odd_100bytes_32align_realloc_naive, 100 - 1, 32);
rt_realloc!(rt_odd_100bytes_32align_realloc, 100 - 1, 32);
rt_realloc_excess_unused!(rt_odd_100bytes_32align_realloc_excess_unused, 100 - 1, 32);
rt_realloc_excess_used!(rt_odd_100bytes_32align_realloc_excess_used, 100 - 1, 32);

rt_calloc!(rt_odd_1000bytes_32align_calloc, 1000 - 1, 32);
rt_mallocx!(rt_odd_1000bytes_32align_mallocx, 1000 - 1, 32);
rt_mallocx_zeroed!(rt_odd_1000bytes_32align_mallocx_zeroed, 1000 - 1, 32);
rt_mallocx_nallocx!(rt_odd_1000bytes_32align_mallocx_nallocx, 1000 - 1, 32);
rt_alloc_layout_checked!(rt_odd_1000bytes_32align_alloc_layout_checked, 1000 - 1, 32);
rt_alloc_layout_unchecked!(
    rt_odd_1000bytes_32align_alloc_layout_unchecked,
    1000 - 1,
    32
);
rt_alloc_excess_unused!(rt_odd_1000bytes_32align_alloc_excess_unused, 1000 - 1, 32);
rt_alloc_excess_used!(rt_odd_1000bytes_32align_alloc_excess_used, 1000 - 1, 32);
rt_realloc_naive!(rt_odd_1000bytes_32align_realloc_naive, 1000 - 1, 32);
rt_realloc!(rt_odd_1000bytes_32align_realloc, 1000 - 1, 32);
rt_realloc_excess_unused!(rt_odd_1000bytes_32align_realloc_excess_unused, 1000 - 1, 32);
rt_realloc_excess_used!(rt_odd_1000bytes_32align_realloc_excess_used, 1000 - 1, 32);

rt_calloc!(rt_odd_10000bytes_32align_calloc, 10000 - 1, 32);
rt_mallocx!(rt_odd_10000bytes_32align_mallocx, 10000 - 1, 32);
rt_mallocx_zeroed!(rt_odd_10000bytes_32align_mallocx_zeroed, 10000 - 1, 32);
rt_mallocx_nallocx!(rt_odd_10000bytes_32align_mallocx_nallocx, 10000 - 1, 32);
rt_alloc_layout_checked!(
    rt_odd_10000bytes_32align_alloc_layout_checked,
    10000 - 1,
    32
);
rt_alloc_layout_unchecked!(
    rt_odd_10000bytes_32align_alloc_layout_unchecked,
    10000 - 1,
    32
);
rt_alloc_excess_unused!(rt_odd_10000bytes_32align_alloc_excess_unused, 10000 - 1, 32);
rt_alloc_excess_used!(rt_odd_10000bytes_32align_alloc_excess_used, 10000 - 1, 32);
rt_realloc_naive!(rt_odd_10000bytes_32align_realloc_naive, 10000 - 1, 32);
rt_realloc!(rt_odd_10000bytes_32align_realloc, 10000 - 1, 32);
rt_realloc_excess_unused!(
    rt_odd_10000bytes_32align_realloc_excess_unused,
    10000 - 1,
    32
);
rt_realloc_excess_used!(rt_odd_10000bytes_32align_realloc_excess_used, 10000 - 1, 32);

rt_calloc!(rt_odd_100000bytes_32align_calloc, 100000 - 1, 32);
rt_mallocx!(rt_odd_100000bytes_32align_mallocx, 100000 - 1, 32);
rt_mallocx_zeroed!(rt_odd_100000bytes_32align_mallocx_zeroed, 100000 - 1, 32);
rt_mallocx_nallocx!(rt_odd_100000bytes_32align_mallocx_nallocx, 100000 - 1, 32);
rt_alloc_layout_checked!(
    rt_odd_100000bytes_32align_alloc_layout_checked,
    100000 - 1,
    32
);
rt_alloc_layout_unchecked!(
    rt_odd_100000bytes_32align_alloc_layout_unchecked,
    100000 - 1,
    32
);
rt_alloc_excess_unused!(
    rt_odd_100000bytes_32align_alloc_excess_unused,
    100000 - 1,
    32
);
rt_alloc_excess_used!(rt_odd_100000bytes_32align_alloc_excess_used, 100000 - 1, 32);
rt_realloc_naive!(rt_odd_100000bytes_32align_realloc_naive, 100000 - 1, 32);
rt_realloc!(rt_odd_100000bytes_32align_realloc, 100000 - 1, 32);
rt_realloc_excess_unused!(
    rt_odd_100000bytes_32align_realloc_excess_unused,
    100000 - 1,
    32
);
rt_realloc_excess_used!(
    rt_odd_100000bytes_32align_realloc_excess_used,
    100000 - 1,
    32
);

rt_calloc!(rt_odd_1000000bytes_32align_calloc, 1000000 - 1, 32);
rt_mallocx!(rt_odd_1000000bytes_32align_mallocx, 1000000 - 1, 32);
rt_mallocx_zeroed!(rt_odd_1000000bytes_32align_mallocx_zeroed, 1000000 - 1, 32);
rt_mallocx_nallocx!(rt_odd_1000000bytes_32align_mallocx_nallocx, 1000000 - 1, 32);
rt_alloc_layout_checked!(
    rt_odd_1000000bytes_32align_alloc_layout_checked,
    1000000 - 1,
    32
);
rt_alloc_layout_unchecked!(
    rt_odd_1000000bytes_32align_alloc_layout_unchecked,
    1000000 - 1,
    32
);
rt_alloc_excess_unused!(
    rt_odd_1000000bytes_32align_alloc_excess_unused,
    1000000 - 1,
    32
);
rt_alloc_excess_used!(
    rt_odd_1000000bytes_32align_alloc_excess_used,
    1000000 - 1,
    32
);
rt_realloc_naive!(rt_odd_1000000bytes_32align_realloc_naive, 1000000 - 1, 32);
rt_realloc!(rt_odd_1000000bytes_32align_realloc, 1000000 - 1, 32);
rt_realloc_excess_unused!(
    rt_odd_1000000bytes_32align_realloc_excess_unused,
    1000000 - 1,
    32
);
rt_realloc_excess_used!(
    rt_odd_1000000bytes_32align_realloc_excess_used,
    1000000 - 1,
    32
);

// primes
rt_calloc!(rt_primes_3bytes_32align_calloc, 3, 32);
rt_mallocx!(rt_primes_3bytes_32align_mallocx, 3, 32);
rt_mallocx_zeroed!(rt_primes_3bytes_32align_mallocx_zeroed, 3, 32);
rt_mallocx_nallocx!(rt_primes_3bytes_32align_mallocx_nallocx, 3, 32);
rt_alloc_layout_checked!(rt_primes_3bytes_32align_alloc_layout_checked, 3, 32);
rt_alloc_layout_unchecked!(rt_primes_3bytes_32align_alloc_layout_unchecked, 3, 32);
rt_alloc_excess_unused!(rt_primes_3bytes_32align_alloc_excess_unused, 3, 32);
rt_alloc_excess_used!(rt_primes_3bytes_32align_alloc_excess_used, 3, 32);
rt_realloc_naive!(rt_primes_3bytes_32align_realloc_naive, 3, 32);
rt_realloc!(rt_primes_3bytes_32align_realloc, 3, 32);
rt_realloc_excess_unused!(rt_primes_3bytes_32align_realloc_excess_unused, 3, 32);
rt_realloc_excess_used!(rt_primes_3bytes_32align_realloc_excess_used, 3, 32);

rt_calloc!(rt_primes_7bytes_32align_calloc, 7, 32);
rt_mallocx!(rt_primes_7bytes_32align_mallocx, 7, 32);
rt_mallocx_zeroed!(rt_primes_7bytes_32align_mallocx_zeroed, 7, 32);
rt_mallocx_nallocx!(rt_primes_7bytes_32align_mallocx_nallocx, 7, 32);
rt_alloc_layout_checked!(rt_primes_7bytes_32align_alloc_layout_checked, 7, 32);
rt_alloc_layout_unchecked!(rt_primes_7bytes_32align_alloc_layout_unchecked, 7, 32);
rt_alloc_excess_unused!(rt_primes_7bytes_32align_alloc_excess_unused, 7, 32);
rt_alloc_excess_used!(rt_primes_7bytes_32align_alloc_excess_used, 7, 32);
rt_realloc_naive!(rt_primes_7bytes_32align_realloc_naive, 7, 32);
rt_realloc!(rt_primes_7bytes_32align_realloc, 7, 32);
rt_realloc_excess_unused!(rt_primes_7bytes_32align_realloc_excess_unused, 7, 32);
rt_realloc_excess_used!(rt_primes_7bytes_32align_realloc_excess_used, 7, 32);

rt_calloc!(rt_primes_13bytes_32align_calloc, 13, 32);
rt_mallocx!(rt_primes_13bytes_32align_mallocx, 13, 32);
rt_mallocx_zeroed!(rt_primes_13bytes_32align_mallocx_zeroed, 13, 32);
rt_mallocx_nallocx!(rt_primes_13bytes_32align_mallocx_nallocx, 13, 32);
rt_alloc_layout_checked!(rt_primes_13bytes_32align_alloc_layout_checked, 13, 32);
rt_alloc_layout_unchecked!(rt_primes_13bytes_32align_alloc_layout_unchecked, 13, 32);
rt_alloc_excess_unused!(rt_primes_13bytes_32align_alloc_excess_unused, 13, 32);
rt_alloc_excess_used!(rt_primes_13bytes_32align_alloc_excess_used, 13, 32);
rt_realloc_naive!(rt_primes_13bytes_32align_realloc_naive, 13, 32);
rt_realloc!(rt_primes_13bytes_32align_realloc, 13, 32);
rt_realloc_excess_unused!(rt_primes_13bytes_32align_realloc_excess_unused, 13, 32);
rt_realloc_excess_used!(rt_primes_13bytes_32align_realloc_excess_used, 13, 32);

rt_calloc!(rt_primes_17bytes_32align_calloc, 17, 32);
rt_mallocx!(rt_primes_17bytes_32align_mallocx, 17, 32);
rt_mallocx_zeroed!(rt_primes_17bytes_32align_mallocx_zeroed, 17, 32);
rt_mallocx_nallocx!(rt_primes_17bytes_32align_mallocx_nallocx, 17, 32);
rt_alloc_layout_checked!(rt_primes_17bytes_32align_alloc_layout_checked, 17, 32);
rt_alloc_layout_unchecked!(rt_primes_17bytes_32align_alloc_layout_unchecked, 17, 32);
rt_alloc_excess_unused!(rt_primes_17bytes_32align_alloc_excess_unused, 17, 32);
rt_alloc_excess_used!(rt_primes_17bytes_32align_alloc_excess_used, 17, 32);
rt_realloc_naive!(rt_primes_17bytes_32align_realloc_naive, 17, 32);
rt_realloc!(rt_primes_17bytes_32align_realloc, 17, 32);
rt_realloc_excess_unused!(rt_primes_17bytes_32align_realloc_excess_unused, 17, 32);
rt_realloc_excess_used!(rt_primes_17bytes_32align_realloc_excess_used, 17, 32);

rt_calloc!(rt_primes_31bytes_32align_calloc, 31, 32);
rt_mallocx!(rt_primes_31bytes_32align_mallocx, 31, 32);
rt_mallocx_zeroed!(rt_primes_31bytes_32align_mallocx_zeroed, 31, 32);
rt_mallocx_nallocx!(rt_primes_31bytes_32align_mallocx_nallocx, 31, 32);
rt_alloc_layout_checked!(rt_primes_31bytes_32align_alloc_layout_checked, 31, 32);
rt_alloc_layout_unchecked!(rt_primes_31bytes_32align_alloc_layout_unchecked, 31, 32);
rt_alloc_excess_unused!(rt_primes_31bytes_32align_alloc_excess_unused, 31, 32);
rt_alloc_excess_used!(rt_primes_31bytes_32align_alloc_excess_used, 31, 32);
rt_realloc_naive!(rt_primes_31bytes_32align_realloc_naive, 31, 32);
rt_realloc!(rt_primes_31bytes_32align_realloc, 31, 32);
rt_realloc_excess_unused!(rt_primes_31bytes_32align_realloc_excess_unused, 31, 32);
rt_realloc_excess_used!(rt_primes_31bytes_32align_realloc_excess_used, 31, 32);

rt_calloc!(rt_primes_61bytes_32align_calloc, 61, 32);
rt_mallocx!(rt_primes_61bytes_32align_mallocx, 61, 32);
rt_mallocx_zeroed!(rt_primes_61bytes_32align_mallocx_zeroed, 61, 32);
rt_mallocx_nallocx!(rt_primes_61bytes_32align_mallocx_nallocx, 61, 32);
rt_alloc_layout_checked!(rt_primes_61bytes_32align_alloc_layout_checked, 61, 32);
rt_alloc_layout_unchecked!(rt_primes_61bytes_32align_alloc_layout_unchecked, 61, 32);
rt_alloc_excess_unused!(rt_primes_61bytes_32align_alloc_excess_unused, 61, 32);
rt_alloc_excess_used!(rt_primes_61bytes_32align_alloc_excess_used, 61, 32);
rt_realloc_naive!(rt_primes_61bytes_32align_realloc_naive, 61, 32);
rt_realloc!(rt_primes_61bytes_32align_realloc, 61, 32);
rt_realloc_excess_unused!(rt_primes_61bytes_32align_realloc_excess_unused, 61, 32);
rt_realloc_excess_used!(rt_primes_61bytes_32align_realloc_excess_used, 61, 32);

rt_calloc!(rt_primes_96bytes_32align_calloc, 96, 32);
rt_mallocx!(rt_primes_96bytes_32align_mallocx, 96, 32);
rt_mallocx_zeroed!(rt_primes_96bytes_32align_mallocx_zeroed, 96, 32);
rt_mallocx_nallocx!(rt_primes_96bytes_32align_mallocx_nallocx, 96, 32);
rt_alloc_layout_checked!(rt_primes_96bytes_32align_alloc_layout_checked, 96, 32);
rt_alloc_layout_unchecked!(rt_primes_96bytes_32align_alloc_layout_unchecked, 96, 32);
rt_alloc_excess_unused!(rt_primes_96bytes_32align_alloc_excess_unused, 96, 32);
rt_alloc_excess_used!(rt_primes_96bytes_32align_alloc_excess_used, 96, 32);
rt_realloc_naive!(rt_primes_96bytes_32align_realloc_naive, 96, 32);
rt_realloc!(rt_primes_96bytes_32align_realloc, 96, 32);
rt_realloc_excess_unused!(rt_primes_96bytes_32align_realloc_excess_unused, 96, 32);
rt_realloc_excess_used!(rt_primes_96bytes_32align_realloc_excess_used, 96, 32);

rt_calloc!(rt_primes_127bytes_32align_calloc, 127, 32);
rt_mallocx!(rt_primes_127bytes_32align_mallocx, 127, 32);
rt_mallocx_zeroed!(rt_primes_127bytes_32align_mallocx_zeroed, 127, 32);
rt_mallocx_nallocx!(rt_primes_127bytes_32align_mallocx_nallocx, 127, 32);
rt_alloc_layout_checked!(rt_primes_127bytes_32align_alloc_layout_checked, 127, 32);
rt_alloc_layout_unchecked!(rt_primes_127bytes_32align_alloc_layout_unchecked, 127, 32);
rt_alloc_excess_unused!(rt_primes_127bytes_32align_alloc_excess_unused, 127, 32);
rt_alloc_excess_used!(rt_primes_127bytes_32align_alloc_excess_used, 127, 32);
rt_realloc_naive!(rt_primes_127bytes_32align_realloc_naive, 127, 32);
rt_realloc!(rt_primes_127bytes_32align_realloc, 127, 32);
rt_realloc_excess_unused!(rt_primes_127bytes_32align_realloc_excess_unused, 127, 32);
rt_realloc_excess_used!(rt_primes_127bytes_32align_realloc_excess_used, 127, 32);

rt_calloc!(rt_primes_257bytes_32align_calloc, 257, 32);
rt_mallocx!(rt_primes_257bytes_32align_mallocx, 257, 32);
rt_mallocx_zeroed!(rt_primes_257bytes_32align_mallocx_zeroed, 257, 32);
rt_mallocx_nallocx!(rt_primes_257bytes_32align_mallocx_nallocx, 257, 32);
rt_alloc_layout_checked!(rt_primes_257bytes_32align_alloc_layout_checked, 257, 32);
rt_alloc_layout_unchecked!(rt_primes_257bytes_32align_alloc_layout_unchecked, 257, 32);
rt_alloc_excess_unused!(rt_primes_257bytes_32align_alloc_excess_unused, 257, 32);
rt_alloc_excess_used!(rt_primes_257bytes_32align_alloc_excess_used, 257, 32);
rt_realloc_naive!(rt_primes_257bytes_32align_realloc_naive, 257, 32);
rt_realloc!(rt_primes_257bytes_32align_realloc, 257, 32);
rt_realloc_excess_unused!(rt_primes_257bytes_32align_realloc_excess_unused, 257, 32);
rt_realloc_excess_used!(rt_primes_257bytes_32align_realloc_excess_used, 257, 32);

rt_calloc!(rt_primes_509bytes_32align_calloc, 509, 32);
rt_mallocx!(rt_primes_509bytes_32align_mallocx, 509, 32);
rt_mallocx_zeroed!(rt_primes_509bytes_32align_mallocx_zeroed, 509, 32);
rt_mallocx_nallocx!(rt_primes_509bytes_32align_mallocx_nallocx, 509, 32);
rt_alloc_layout_checked!(rt_primes_509bytes_32align_alloc_layout_checked, 509, 32);
rt_alloc_layout_unchecked!(rt_primes_509bytes_32align_alloc_layout_unchecked, 509, 32);
rt_alloc_excess_unused!(rt_primes_509bytes_32align_alloc_excess_unused, 509, 32);
rt_alloc_excess_used!(rt_primes_509bytes_32align_alloc_excess_used, 509, 32);
rt_realloc_naive!(rt_primes_509bytes_32align_realloc_naive, 509, 32);
rt_realloc!(rt_primes_509bytes_32align_realloc, 509, 32);
rt_realloc_excess_unused!(rt_primes_509bytes_32align_realloc_excess_unused, 509, 32);
rt_realloc_excess_used!(rt_primes_509bytes_32align_realloc_excess_used, 509, 32);

rt_calloc!(rt_primes_1021bytes_32align_calloc, 1021, 32);
rt_mallocx!(rt_primes_1021bytes_32align_mallocx, 1021, 32);
rt_mallocx_zeroed!(rt_primes_1021bytes_32align_mallocx_zeroed, 1021, 32);
rt_mallocx_nallocx!(rt_primes_1021bytes_32align_mallocx_nallocx, 1021, 32);
rt_alloc_layout_checked!(rt_primes_1021bytes_32align_alloc_layout_checked, 1021, 32);
rt_alloc_layout_unchecked!(rt_primes_1021bytes_32align_alloc_layout_unchecked, 1021, 32);
rt_alloc_excess_unused!(rt_primes_1021bytes_32align_alloc_excess_unused, 1021, 32);
rt_alloc_excess_used!(rt_primes_1021bytes_32align_alloc_excess_used, 1021, 32);
rt_realloc_naive!(rt_primes_1021bytes_32align_realloc_naive, 1021, 32);
rt_realloc!(rt_primes_1021bytes_32align_realloc, 1021, 32);
rt_realloc_excess_unused!(rt_primes_1021bytes_32align_realloc_excess_unused, 1021, 32);
rt_realloc_excess_used!(rt_primes_1021bytes_32align_realloc_excess_used, 1021, 32);

rt_calloc!(rt_primes_2039bytes_32align_calloc, 2039, 32);
rt_mallocx!(rt_primes_2039bytes_32align_mallocx, 2039, 32);
rt_mallocx_zeroed!(rt_primes_2039bytes_32align_mallocx_zeroed, 2039, 32);
rt_mallocx_nallocx!(rt_primes_2039bytes_32align_mallocx_nallocx, 2039, 32);
rt_alloc_layout_checked!(rt_primes_2039bytes_32align_alloc_layout_checked, 2039, 32);
rt_alloc_layout_unchecked!(rt_primes_2039bytes_32align_alloc_layout_unchecked, 2039, 32);
rt_alloc_excess_unused!(rt_primes_2039bytes_32align_alloc_excess_unused, 2039, 32);
rt_alloc_excess_used!(rt_primes_2039bytes_32align_alloc_excess_used, 2039, 32);
rt_realloc_naive!(rt_primes_2039bytes_32align_realloc_naive, 2039, 32);
rt_realloc!(rt_primes_2039bytes_32align_realloc, 2039, 32);
rt_realloc_excess_unused!(rt_primes_2039bytes_32align_realloc_excess_unused, 2039, 32);
rt_realloc_excess_used!(rt_primes_2039bytes_32align_realloc_excess_used, 2039, 32);

rt_calloc!(rt_primes_4093bytes_32align_calloc, 4093, 32);
rt_mallocx!(rt_primes_4093bytes_32align_mallocx, 4093, 32);
rt_mallocx_zeroed!(rt_primes_4093bytes_32align_mallocx_zeroed, 4093, 32);
rt_mallocx_nallocx!(rt_primes_4093bytes_32align_mallocx_nallocx, 4093, 32);
rt_alloc_layout_checked!(rt_primes_4093bytes_32align_alloc_layout_checked, 4093, 32);
rt_alloc_layout_unchecked!(rt_primes_4093bytes_32align_alloc_layout_unchecked, 4093, 32);
rt_alloc_excess_unused!(rt_primes_4093bytes_32align_alloc_excess_unused, 4093, 32);
rt_alloc_excess_used!(rt_primes_4093bytes_32align_alloc_excess_used, 4093, 32);
rt_realloc_naive!(rt_primes_4093bytes_32align_realloc_naive, 4093, 32);
rt_realloc!(rt_primes_4093bytes_32align_realloc, 4093, 32);
rt_realloc_excess_unused!(rt_primes_4093bytes_32align_realloc_excess_unused, 4093, 32);
rt_realloc_excess_used!(rt_primes_4093bytes_32align_realloc_excess_used, 4093, 32);

rt_calloc!(rt_primes_8191bytes_32align_calloc, 8191, 32);
rt_mallocx!(rt_primes_8191bytes_32align_mallocx, 8191, 32);
rt_mallocx_zeroed!(rt_primes_8191bytes_32align_mallocx_zeroed, 8191, 32);
rt_mallocx_nallocx!(rt_primes_8191bytes_32align_mallocx_nallocx, 8191, 32);
rt_alloc_layout_checked!(rt_primes_8191bytes_32align_alloc_layout_checked, 8191, 32);
rt_alloc_layout_unchecked!(rt_primes_8191bytes_32align_alloc_layout_unchecked, 8191, 32);
rt_alloc_excess_unused!(rt_primes_8191bytes_32align_alloc_excess_unused, 8191, 32);
rt_alloc_excess_used!(rt_primes_8191bytes_32align_alloc_excess_used, 8191, 32);
rt_realloc_naive!(rt_primes_8191bytes_32align_realloc_naive, 8191, 32);
rt_realloc!(rt_primes_8191bytes_32align_realloc, 8191, 32);
rt_realloc_excess_unused!(rt_primes_8191bytes_32align_realloc_excess_unused, 8191, 32);
rt_realloc_excess_used!(rt_primes_8191bytes_32align_realloc_excess_used, 8191, 32);

rt_calloc!(rt_primes_16381bytes_32align_calloc, 16381, 32);
rt_mallocx!(rt_primes_16381bytes_32align_mallocx, 16381, 32);
rt_mallocx_zeroed!(rt_primes_16381bytes_32align_mallocx_zeroed, 16381, 32);
rt_mallocx_nallocx!(rt_primes_16381bytes_32align_mallocx_nallocx, 16381, 32);
rt_alloc_layout_checked!(rt_primes_16381bytes_32align_alloc_layout_checked, 16381, 32);
rt_alloc_layout_unchecked!(
    rt_primes_16381bytes_32align_alloc_layout_unchecked,
    16381,
    32
);
rt_alloc_excess_unused!(rt_primes_16381bytes_32align_alloc_excess_unused, 16381, 32);
rt_alloc_excess_used!(rt_primes_16381bytes_32align_alloc_excess_used, 16381, 32);
rt_realloc_naive!(rt_primes_16381bytes_32align_realloc_naive, 16381, 32);
rt_realloc!(rt_primes_16381bytes_32align_realloc, 16381, 32);
rt_realloc_excess_unused!(
    rt_primes_16381bytes_32align_realloc_excess_unused,
    16381,
    32
);
rt_realloc_excess_used!(rt_primes_16381bytes_32align_realloc_excess_used, 16381, 32);

rt_calloc!(rt_primes_32749bytes_32align_calloc, 32749, 32);
rt_mallocx!(rt_primes_32749bytes_32align_mallocx, 32749, 32);
rt_mallocx_zeroed!(rt_primes_32749bytes_32align_mallocx_zeroed, 32749, 32);
rt_mallocx_nallocx!(rt_primes_32749bytes_32align_mallocx_nallocx, 32749, 32);
rt_alloc_layout_checked!(rt_primes_32749bytes_32align_alloc_layout_checked, 32749, 32);
rt_alloc_layout_unchecked!(
    rt_primes_32749bytes_32align_alloc_layout_unchecked,
    32749,
    32
);
rt_alloc_excess_unused!(rt_primes_32749bytes_32align_alloc_excess_unused, 32749, 32);
rt_alloc_excess_used!(rt_primes_32749bytes_32align_alloc_excess_used, 32749, 32);
rt_realloc_naive!(rt_primes_32749bytes_32align_realloc_naive, 32749, 32);
rt_realloc!(rt_primes_32749bytes_32align_realloc, 32749, 32);
rt_realloc_excess_unused!(
    rt_primes_32749bytes_32align_realloc_excess_unused,
    32749,
    32
);
rt_realloc_excess_used!(rt_primes_32749bytes_32align_realloc_excess_used, 32749, 32);

rt_calloc!(rt_primes_65537bytes_32align_calloc, 65537, 32);
rt_mallocx!(rt_primes_65537bytes_32align_mallocx, 65537, 32);
rt_mallocx_zeroed!(rt_primes_65537bytes_32align_mallocx_zeroed, 65537, 32);
rt_mallocx_nallocx!(rt_primes_65537bytes_32align_mallocx_nallocx, 65537, 32);
rt_alloc_layout_checked!(rt_primes_65537bytes_32align_alloc_layout_checked, 65537, 32);
rt_alloc_layout_unchecked!(
    rt_primes_65537bytes_32align_alloc_layout_unchecked,
    65537,
    32
);
rt_alloc_excess_unused!(rt_primes_65537bytes_32align_alloc_excess_unused, 65537, 32);
rt_alloc_excess_used!(rt_primes_65537bytes_32align_alloc_excess_used, 65537, 32);
rt_realloc_naive!(rt_primes_65537bytes_32align_realloc_naive, 65537, 32);
rt_realloc!(rt_primes_65537bytes_32align_realloc, 65537, 32);
rt_realloc_excess_unused!(
    rt_primes_65537bytes_32align_realloc_excess_unused,
    65537,
    32
);
rt_realloc_excess_used!(rt_primes_65537bytes_32align_realloc_excess_used, 65537, 32);

rt_calloc!(rt_primes_131071bytes_32align_calloc, 131071, 32);
rt_mallocx!(rt_primes_131071bytes_32align_mallocx, 131071, 32);
rt_mallocx_zeroed!(rt_primes_131071bytes_32align_mallocx_zeroed, 131071, 32);
rt_mallocx_nallocx!(rt_primes_131071bytes_32align_mallocx_nallocx, 131071, 32);
rt_alloc_layout_checked!(
    rt_primes_131071bytes_32align_alloc_layout_checked,
    131071,
    32
);
rt_alloc_layout_unchecked!(
    rt_primes_131071bytes_32align_alloc_layout_unchecked,
    131071,
    32
);
rt_alloc_excess_unused!(
    rt_primes_131071bytes_32align_alloc_excess_unused,
    131071,
    32
);
rt_alloc_excess_used!(rt_primes_131071bytes_32align_alloc_excess_used, 131071, 32);
rt_realloc_naive!(rt_primes_131071bytes_32align_realloc_naive, 131071, 32);
rt_realloc!(rt_primes_131071bytes_32align_realloc, 131071, 32);
rt_realloc_excess_unused!(
    rt_primes_131071bytes_32align_realloc_excess_unused,
    131071,
    32
);
rt_realloc_excess_used!(
    rt_primes_131071bytes_32align_realloc_excess_used,
    131071,
    32
);

rt_calloc!(rt_primes_4194301bytes_32align_calloc, 4194301, 32);
rt_mallocx!(rt_primes_4194301bytes_32align_mallocx, 4194301, 32);
rt_mallocx_zeroed!(rt_primes_4194301bytes_32align_mallocx_zeroed, 4194301, 32);
rt_mallocx_nallocx!(rt_primes_4194301bytes_32align_mallocx_nallocx, 4194301, 32);
rt_alloc_layout_checked!(
    rt_primes_4194301bytes_32align_alloc_layout_checked,
    4194301,
    32
);
rt_alloc_layout_unchecked!(
    rt_primes_4194301bytes_32align_alloc_layout_unchecked,
    4194301,
    32
);
rt_alloc_excess_unused!(
    rt_primes_4194301bytes_32align_alloc_excess_unused,
    4194301,
    32
);
rt_alloc_excess_used!(
    rt_primes_4194301bytes_32align_alloc_excess_used,
    4194301,
    32
);
rt_realloc_naive!(rt_primes_4194301bytes_32align_realloc_naive, 4194301, 32);
rt_realloc!(rt_primes_4194301bytes_32align_realloc, 4194301, 32);
rt_realloc_excess_unused!(
    rt_primes_4194301bytes_32align_realloc_excess_unused,
    4194301,
    32
);
rt_realloc_excess_used!(
    rt_primes_4194301bytes_32align_realloc_excess_used,
    4194301,
    32
);
