//! Rust bindings to the `jemalloc` C library.
//!
//! `jemalloc` is a general purpose memory allocation, its documentation
//! can be found here:
//!
//! * [API documentation][jemalloc_docs]
//! * [Wiki][jemalloc_wiki] (design documents, presentations, profiling, debugging, tuning, ...)
//!
//! `jemalloc` exposes both a standard and a non-standard API.
//!
//! # Standard API
//!
//! The standard API includes: the [`malloc`], [`calloc`], [`realloc`], and
//! [`free`], which conform to to ISO/IEC 9899:1990 (“ISO C90”),
//! [`posix_memalign`] which conforms to conforms to POSIX.1-2016, and
//! [`aligned_alloc`].
//!
//! Note that these standard leave some details as _implementation defined_.
//! This docs document this behavior for `jemalloc`, but keep in mind that other
//! standard-conforming implementations of these functions in other allocators
//! might behave slightly different.
//!
//! # Non-Standard API
//!
//! The non-standard API includes: [`mallocx`], [`rallocx`], [`xallocx`],
//! [`sallocx`], [`dallocx`], [`sdallocx`], and [`nallocx`]. These functions all
//! have a `flags` argument that can be used to specify options. Use bitwise or
//! `|` to specify one or more of the following: [`MALLOCX_LG_ALIGN`],
//! [`MALLOCX_ALIGN`], [`MALLOCX_ZERO`], [`MALLOCX_TCACHE`],
//! [`MALLOCX_TCACHE_NONE`], and [`MALLOCX_ARENA`].
//!
//! # Environment variables
//!
//! The `MALLOC_CONF` environment variable affects the execution of the allocation functions.
//!
//! For the documentation of the [`MALLCTL` namespace visit the jemalloc
//! documenation][jemalloc_mallctl].
//!
//! [jemalloc_docs]: http://jemalloc.net/jemalloc.3.html
//! [jemalloc_wiki]: https://github.com/jemalloc/jemalloc/wiki
//! [jemalloc_mallctl]: http://jemalloc.net/jemalloc.3.html#mallctl_namespace
#![no_std]
#![allow(non_snake_case, non_camel_case_types)]

extern crate libc;

use libc::{c_char, c_int, c_uint, c_void, size_t};
type c_bool = c_int;

/// Align the memory allocation to start at an address that is a
/// multiple of `1 << la`.
///
/// # Safety
///
/// It does not validate that `la` is within the valid range.
#[inline]
pub fn MALLOCX_LG_ALIGN(la: usize) -> c_int {
    la as c_int
}

/// Align the memory allocation to start at an address that is a multiple of `align`,
/// where a is a power of two.
///
/// # Safety
///
/// This macro does not validate that a is a power of 2.
#[inline]
pub fn MALLOCX_ALIGN(aling: usize) -> c_int {
    aling.trailing_zeros() as c_int
}

/// Initialize newly allocated memory to contain zero bytes.
///
/// In the growing reallocation case, the real size prior to reallocation
/// defines the boundary between untouched bytes and those that are initialized
/// to contain zero bytes.
///
/// If this option is not set, newly allocated memory is uninitialized.
pub const MALLOCX_ZERO: c_int = 0x40;

/// Use the thread-specific cache (_tcache_) specified by the identifier `tc`.
///
/// # Safety
///
/// `tc` must have been acquired via the `tcache.create mallctl`. This function
/// does not validate that `tc` specifies a valid identifier.
#[inline]
pub fn MALLOCX_TCACHE(tc: usize) -> c_int {
    tc.wrapping_add(2).wrapping_shl(8) as c_int
}

/// Do not use a thread-specific cache (_tcache_).
///
/// Unless `MALLOCX_TCACHE(tc)` or `MALLOCX_TCACHE_NONE` is specified, an
/// automatically managed _tcache_ will be used under many circumstances.
///
/// # Safety
///
/// This option cannot be used in the same `flags` argument as
/// `MALLOCX_TCACHE(tc)`.
// FIXME: This should just be a const.
#[inline]
pub fn MALLOCX_TCACHE_NONE() -> c_int {
    MALLOCX_TCACHE(!0)
}

/// Use the arena specified by the index `a`.
///
/// This option has no effect for regions that were allocated via an arena other
/// than the one specified.
///
/// # Safety
///
/// This function does not validate that `a` specifies an arena index in the
/// valid range.
#[inline]
pub fn MALLOCX_ARENA(a: usize) -> c_int {
    (a as c_int).wrapping_add(1).wrapping_shl(20)
}

extern "C" {
    /// Allocates `size` bytes of uninitialized memory.
    ///
    /// It returns a pointer to the start (lowest byte address) of the allocated
    /// space. This pointer is suitably aligned so that it may be assigned to a
    /// pointer to any type of object and then used to access such an object in
    /// the space allocated until the space is explicitly deallocated. Each
    /// yielded pointer points to an object disjoint from any other object.
    ///
    /// If the `size` of the space requested is zero, either a null pointer is
    /// returned, or the behavior is as if the `size` were some nonzero value,
    /// except that the returned pointer shall not be used to access an object.
    ///
    /// # Errors
    ///
    /// If the space cannot be allocated, a null pointer is returned and `errno`
    /// is set to `ENOMEM`.
    #[cfg_attr(prefixed, link_name = "_rjem_malloc")]
    pub fn malloc(size: size_t) -> *mut c_void;
    /// Allocates zero-initialized space for an array of `number` objects, each
    /// of whose size is `size`.
    ///
    /// The result is identical to calling [`malloc`] with an argument of
    /// `number * size`, with the exception that the allocated memory is
    /// explicitly initialized to _zero_ bytes.
    ///
    /// Note: zero-initialized memory need not be the same as the
    /// representation of floating-point zero or a null pointer constant.
    #[cfg_attr(prefixed, link_name = "_rjem_calloc")]
    pub fn calloc(number: size_t, size: size_t) -> *mut c_void;

    /// Allocates `size` bytes of memory at an address which is a multiple of
    /// `alignment` and is placed in `*ptr`.
    ///
    /// If `size` is zero, then the value placed in `*ptr` is either null, or
    /// the behavior is as if the `size` were some nonzero value, except that
    /// the returned pointer shall not be used to access an object.
    ///
    /// # Errors
    ///
    /// On success, it returns zero. On error, the value of `errno` is _not_ set,
    /// `*ptr` is not modified, and the return values can be:
    ///
    /// - `EINVAL`: the `alignment` argument was not a power-of-two or was not a multiple of
    ///   `mem::size_of::<*const c_void>()`.
    /// - `ENOMEM`: there was insufficient memory to fulfill the allocation request.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `ptr` is null.
    #[cfg_attr(prefixed, link_name = "_rjem_posix_memalign")]
    pub fn posix_memalign(ptr: *mut *mut c_void, alignment: size_t, size: size_t) -> c_int;

    /// Allocates `size` bytes of memory at an address which is a multiple of
    /// `alignment`.
    ///
    /// If the `size` of the space requested is zero, either a null pointer is
    /// returned, or the behavior is as if the `size` were some nonzero value,
    /// except that the returned pointer shall not be used to access an object.
    ///
    /// # Errors
    ///
    /// Returns null if the request fails.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `alignment` is not a power-of-two
    /// * `size` is not an integral multiple of `alignment`
    #[cfg_attr(prefixed, link_name = "_rjem_aligned_alloc")]
    pub fn aligned_alloc(alignment: size_t, size: size_t) -> *mut c_void;

    /// Resizes the previously-allocated memory region referenced by `ptr` to
    /// `size` bytes.
    ///
    /// Deallocates the old object pointed to by `ptr` and returns a pointer to
    /// a new object that has the size specified by `size`. The contents of the
    /// new object are the same as that of the old object prior to deallocation,
    /// up to the lesser of the new and old sizes.
    ///
    /// The memory in the new object beyond the size of the old object is
    /// uninitialized.
    ///
    /// The returned pointer to a new object may have the same value as a
    /// pointer to the old object, but [`realloc`] may move the memory
    /// allocation, resulting in a different return value than `ptr`.
    ///
    /// If `ptr` is null, [`realloc`] behaves identically to [`malloc`] for the
    /// specified size.
    ///
    /// If the size of the space requested is zero, the behavior is
    /// implementation-defined: either a null pointer is returned, or the
    /// behavior is as if the size were some nonzero value, except that the
    /// returned pointer shall not be used to access an object # Errors
    ///
    /// # Errors
    ///
    /// If memory for the new object cannot be allocated, the old object is not
    /// deallocated, its value is unchanged, [`realloc`] returns null, and
    /// `errno` is set to `ENOMEM`.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `ptr` does not match a pointer previously returned by the memory
    ///   allocation functions of this crate, or
    /// * the memory region referenced by `ptr` has been deallocated.
    #[cfg_attr(prefixed, link_name = "_rjem_realloc")]
    pub fn realloc(ptr: *mut c_void, size: size_t) -> *mut c_void;

    /// Deallocates previously-allocated memory region referenced by `ptr`.
    ///
    /// This makes the space available for future allocations.
    ///
    /// If `ptr` is null, no action occurs.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `ptr` does not match a pointer earlier returned by the memory
    ///   allocation functions of this crate, or
    /// * the memory region referenced by `ptr` has been deallocated.
    #[cfg_attr(prefixed, link_name = "_rjem_free")]
    pub fn free(ptr: *mut c_void);

    /// Allocates at least `size` bytes of memory according to `flags`.
    ///
    /// It returns a pointer to the start (lowest byte address) of the allocated
    /// space. This pointer is suitably aligned so that it may be assigned to a
    /// pointer to any type of object and then used to access such an object in
    /// the space allocated until the space is explicitly deallocated. Each
    /// yielded pointer points to an object disjoint from any other object.
    ///
    /// # Errors
    ///
    /// On success it returns a non-null pointer. A null pointer return value
    /// indicates that insufficient contiguous memory was available to service
    /// the allocation request.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if `size == 0`.
    #[cfg_attr(prefixed, link_name = "_rjem_mallocx")]
    pub fn mallocx(size: size_t, flags: c_int) -> *mut c_void;

    /// Resizes the previously-allocated memory region referenced by `ptr` to be
    /// at least `size` bytes.
    ///
    /// Deallocates the old object pointed to by `ptr` and returns a pointer to
    /// a new object that has the size specified by `size`. The contents of the
    /// new object are the same as that of the old object prior to deallocation,
    /// up to the lesser of the new and old sizes.
    ///
    /// The the memory in the new object beyond the size of the old object is
    /// obtained according to `flags` (it might be uninitialized).
    ///
    /// The returned pointer to a new object may have the same value as a
    /// pointer to the old object, but [`rallocx`] may move the memory
    /// allocation, resulting in a different return value than `ptr`.
    ///
    /// # Errors
    ///
    /// On success it returns a non-null pointer. A null pointer return value
    /// indicates that insufficient contiguous memory was available to service
    /// the allocation request. In this case, the old object is not
    /// deallocated, and its value is unchanged.
    ///
    /// # Safety
    ///
    /// The behavior is _undefiend_ if:
    ///
    /// * `size == 0`, or
    /// * `ptr` does not match a pointer earlier returned by
    ///   the memory allocation functions of this crate, or
    /// * the memory region referenced by `ptr` has been deallocated.
    #[cfg_attr(prefixed, link_name = "_rjem_rallocx")]
    pub fn rallocx(ptr: *mut c_void, size: size_t, flags: c_int) -> *mut c_void;

    /// Resizes the previously-allocated memory region referenced by `ptr` _in
    /// place_ to be at least `size` bytes, returning the real size of the
    /// allocation.
    ///
    /// Deallocates the old object pointed to by `ptr` and sets `ptr` to a new
    /// object that has the size returned; the old a new objects share the same
    /// base address. The contents of the new object are the same as that of the
    /// old object prior to deallocation, up to the lesser of the new and old
    /// sizes.
    ///
    /// If `extra` is non-zero, an attempt is made to resize the allocation to
    /// be at least `size + extra` bytes. Inability to allocate the `extra`
    /// bytes will not by itself result in failure to resize.
    ///
    /// The memory in the new object beyond the size of the old object is
    /// obtained according to `flags` (it might be uninitialized).
    ///
    /// # Errors
    ///
    /// If the allocation cannot be adequately grown in place up to `size`, the
    /// size returned is smaller than `size`.
    ///
    /// Note:
    ///
    /// * the size value returned can be larger than the size requested during
    ///   allocation
    /// * when shrinking an allocation, use the size returned to determine
    ///   whether the allocation was shrunk sufficiently or not.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `size == 0`, or
    /// * `size + extra > size_t::max_value()`, or
    /// * `ptr` does not match a pointer earlier returned by the memory
    ///   allocation functions of this crate, or
    /// * the memory region referenced by `ptr` has been deallocated.
    #[cfg_attr(prefixed, link_name = "_rjem_xallocx")]
    pub fn xallocx(ptr: *mut c_void, size: size_t, extra: size_t, flags: c_int) -> size_t;

    /// Returns the real size of the previously-allocated memory region
    /// referenced by `ptr`.
    ///
    /// The value may be larger than the size requested on allocation.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `ptr` does not match a pointer earlier returned by the memory
    ///   allocation functions of this crate, or
    /// * the memory region referenced by `ptr` has been deallocated.
    #[cfg_attr(prefixed, link_name = "_rjem_sallocx")]
    pub fn sallocx(ptr: *const c_void, flags: c_int) -> size_t;

    /// Deallocates previously-allocated memory region referenced by `ptr`.
    ///
    /// This makes the space available for future allocations.
    ///
    /// If `ptr` is null, no action occurs.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `ptr` does not match a pointer earlier returned by the memory
    ///   allocation functions of this crate, or
    /// * the memory region referenced by `ptr` has been deallocated.
    #[cfg_attr(prefixed, link_name = "_rjem_dallocx")]
    pub fn dallocx(ptr: *mut c_void, flags: c_int);

    /// Deallocates previously-allocated memory region referenced by `ptr` with
    /// `size` hint.
    ///
    /// This makes the space available for future allocations.
    ///
    /// If `ptr` is null, no action occurs.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `size` is not in range `[req_size, alloc_size]`, where `req_size` is
    /// the size requested when performing the allocation, and `alloc_size` is
    /// the allocation size returned by [`nallocx`], [`sallocx`], or
    /// [`xallocx`],
    /// * `ptr` does not match a pointer earlier returned by the memory
    ///   allocation functions of this crate, or
    /// * the memory region referenced by `ptr` has been deallocated.
    #[cfg_attr(prefixed, link_name = "_rjem_sdallocx")]
    pub fn sdallocx(ptr: *mut c_void, size: size_t, flags: c_int);

    /// Returns the real size of the allocation that would result from a
    /// [`mallocx`] function call with the same arguments.
    ///
    /// # Errors
    ///
    /// If the inputs exceed the maximum supported size class and/or alignment
    /// it returns zero.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if `size == 0`.
    #[cfg_attr(prefixed, link_name = "_rjem_nallocx")]
    pub fn nallocx(size: size_t, flags: c_int) -> size_t;

    /// Returns the real size of the previously-allocated memory region
    /// referenced by `ptr`.
    ///
    /// The value may be larger than the size requested on allocation.
    ///
    /// Although the excess bytes can be overwritten by the application without
    /// ill effects, this is not good programming practice: the number of excess
    /// bytes in an allocation depends on the underlying implementation.
    ///
    /// The main use of this function is for debugging and introspection.
    ///
    /// # Errors
    ///
    /// If `ptr` is null, 0 is returned.
    ///
    /// # Safety
    ///
    /// The behavior is _undefined_ if:
    ///
    /// * `ptr` does not match a pointer earlier returned by the memory
    ///   allocation functions of this crate, or
    /// * the memory region referenced by `ptr` has been deallocated.
    #[cfg_attr(prefixed, link_name = "_rjem_malloc_usable_size")]
    pub fn malloc_usable_size(ptr: *const c_void) -> size_t;

    /// General interface for introspecting the memory allocator, as well as
    /// setting modifiable parameters and triggering actions.
    ///
    /// The period-separated name argument specifies a location in a
    /// tree-structured namespace ([see jemalloc's `MALLCTL`
    /// documentation][jemalloc_mallctl]).
    ///
    /// To read a value, pass a pointer via `oldp` to adequate space to contain
    /// the value, and a pointer to its length via `oldlenp``; otherwise pass
    /// null and null. Similarly, to write a value, pass a pointer to the value
    /// via `newp`, and its length via `newlen`; otherwise pass null and 0.
    ///
    /// # Errors
    ///
    /// Returns `0` on success, otherwise returns:
    ///
    /// * `EINVAL`: if `newp` is not null, and `newlen` is too large or too
    /// small. Alternatively, `*oldlenp` is too large or too small; in this case
    /// as much data as possible are read despite the error.
    ///
    /// * `ENOENT`: `name` or mib specifies an unknown/invalid value.
    ///
    /// * `EPERM`: Attempt to read or write void value, or attempt to write read-only value.
    ///
    /// * `EAGAIN`: A memory allocation failure occurred.
    ///
    /// * `EFAULT`: An interface with side effects failed in some way not
    /// directly related to `mallctl` read/write processing.
    ///
    /// [jemalloc_mallctl]: http://jemalloc.net/jemalloc.3.html#mallctl_namespace
    #[cfg_attr(prefixed, link_name = "_rjem_mallctl")]
    pub fn mallctl(
        name: *const c_char,
        oldp: *mut c_void,
        oldlenp: *mut size_t,
        newp: *mut c_void,
        newlen: size_t,
    ) -> c_int;
    /// Translates a name to a “Management Information Base” (MIB) that can be
    /// passed repeatedly to [`mallctlbymib`].
    ///
    /// This avoids repeated name lookups for applications that repeatedly query
    /// the same portion of the namespace.
    ///
    /// On success, `mibp` contains an array of `*miblenp` integers, where
    /// `*miblenp` is the lesser of the number of components in name and the
    /// input value of `*miblenp`. Thus it is possible to pass a `*miblenp` that is
    /// smaller than the number of period-separated name components, which
    /// results in a partial MIB that can be used as the basis for constructing
    /// a complete MIB. For name components that are integers (e.g. the 2 in
    /// arenas.bin.2.size), the corresponding MIB component will always be that
    /// integer.
    #[cfg_attr(prefixed, link_name = "_rjem_mallctlnametomib")]
    pub fn mallctlnametomib(name: *const c_char, mibp: *mut size_t, miblenp: *mut size_t) -> c_int;

    /// Like [`mallctl`] but taking a `mib` as input instead of a name.
    #[cfg_attr(prefixed, link_name = "_rjem_mallctlbymib")]
    pub fn mallctlbymib(
        mib: *const size_t,
        miblen: size_t,
        oldp: *mut c_void,
        oldpenp: *mut size_t,
        newp: *mut c_void,
        newlen: size_t,
    ) -> c_int;

    /// Writes summary statistics via the `write_cb` callback function pointer
    /// and `cbopaque` data passed to `write_cb`, or [`malloc_message`] if `write_cb`
    /// is null.
    ///
    /// The statistics are presented in human-readable form unless “J”
    /// is specified as a character within the opts string, in which case the
    /// statistics are presented in JSON format.
    ///
    /// This function can be called repeatedly.
    ///
    /// General information that never changes during execution can be omitted
    /// by specifying `g` as a character within the opts string.
    ///
    /// Note that [`malloc_message`] uses the `mallctl*` functions internally,
    /// so inconsistent statistics can be reported if multiple threads use these
    /// functions simultaneously.
    ///
    /// If the Cargo feature `stats` is enabled, `m`, `d`, and `a` can be
    /// specified to omit merged arena, destroyed merged arena, and per arena
    /// statistics, respectively; `b` and `l` can be specified to omit per size
    /// class statistics for bins and large objects, respectively; `x` can be
    /// specified to omit all mutex statistics. Unrecognized characters are
    /// silently ignored.
    ///
    /// Note that thread caching may prevent some statistics from being
    /// completely up to date, since extra locking would be required to merge
    /// counters that track thread cache operations.
    #[cfg_attr(prefixed, link_name = "_rjem_malloc_stats_print")]
    pub fn malloc_stats_print(
        write_cb: extern "C" fn(*mut c_void, *const c_char),
        cbopaque: *mut c_void,
        opts: *const c_char,
    );

    /// Allows overriding the function which emits the text strings forming the
    /// errors and warnings if for some reason the `STDERR_FILENO` file descriptor
    /// is not suitable for this.
    ///
    /// [`malloc_message`] takes the `cbopaque` pointer argument that is null,
    /// unless overridden by the arguments in a call to [`malloc_stats_print`],
    /// followed by a string pointer.
    ///
    /// Please note that doing anything which tries to allocate memory in this
    /// function is likely to result in a crash or deadlock.
    #[no_mangle]
    pub static mut malloc_message: extern "C" fn(cbopaque: *mut c_void, s: *const c_char);
}

/// Extent lifetime management functions.
pub type extent_hooks_t = extent_hooks_s;

/// Extent lifetime management functions.
#[repr(C)]
pub struct extent_hooks_s {
    pub alloc: *mut extent_alloc_t,
    pub dalloc: *mut extent_dalloc_t,
    pub destroy: *mut extent_destroy_t,
    pub commit: *mut extent_commit_t,
    pub decommit: *mut extent_decommit_t,
    pub purge_lazy: *mut extent_purge_t,
    pub purge_forced: *mut extent_purge_t,
    pub split: *mut extent_split_t,
    pub merge: *mut extent_merge_t,
}

/// Extent allocation function.
///
/// On success returns a pointer to `size` bytes of mapped memory on behalf of
/// arena `arena_ind` such that the extent's base address is a multiple of
/// `alignment`, as well as setting `*zero` to indicate whether the extent is
/// zeroed and `*commit` to indicate whether the extent is committed.
///
/// Zeroing is mandatory if `*zero` is `true` upon function entry. Committing is mandatory if
/// `*commit` is true upon function entry. If `new_addr` is not null, the returned
/// pointer must be `new_addr` on success or null on error.
///
/// Committed memory may be committed in absolute terms as on a system that does
/// not overcommit, or in implicit terms as on a system that overcommits and
/// satisfies physical memory needs on demand via soft page faults. Note that
/// replacing the default extent allocation function makes the arena's
/// `arena.<i>.dss` setting irrelevant.
///
/// # Errors
///
/// On error the function returns null and leaves `*zero` and `*commit` unmodified.
///
/// # Safety
///
/// The behavior is _undefined_ if:
///
/// * the `size` parameter is not a multiple of the page size
/// * the `alignment` parameter is not a power of two at least as large as the page size
pub type extent_alloc_t = extern "C" fn(
    extent_hooks: *mut extent_hooks_t,
    new_addr: *mut c_void,
    size: size_t,
    alignment: size_t,
    zero: *mut c_bool,
    commit: *mut c_bool,
    arena_ind: c_uint,
) -> *mut c_void;

/// Extent deallocation function.
///
/// Deallocates an extent at given `addr` and `size` with `committed`/decommited
/// memory as indicated, on behalf of arena `arena_ind`, returning `false` upon
/// success.
///
/// If the function returns `true`, this indicates opt-out from deallocation;
/// the virtual memory mapping associated with the extent remains mapped, in the
/// same commit state, and available for future use, in which case it will be
/// automatically retained for later reuse.
pub type extent_dalloc_t = extern "C" fn(
    extent_hooks: *mut extent_hooks_t,
    addr: *mut c_void,
    size: size_t,
    committed: c_bool,
    arena_ind: c_uint,
) -> c_bool;

/// Extent destruction function.
///
/// Unconditionally destroys an extent at given `addr` and `size` with
/// `committed`/decommited memory as indicated, on behalf of arena `arena_ind`.
///
/// This function may be called to destroy retained extents during arena
/// destruction (see `arena.<i>.destroy`).
pub type extent_destroy_t = extern "C" fn(
    extent_hooks: *mut extent_hooks_t,
    addr: *mut c_void,
    size: size_t,
    committed: c_bool,
    arena_ind: c_uint,
);

/// Extent commit function.
///
/// Commits zeroed physical memory to back pages within an extent at given
/// `addr` and `size` at `offset` bytes, extending for `length` on behalf of
/// arena `arena_ind`, returning `false` upon success.
///
/// Committed memory may be committed in absolute terms as on a system that does
/// not overcommit, or in implicit terms as on a system that overcommits and
/// satisfies physical memory needs on demand via soft page faults. If the
/// function returns `true`, this indicates insufficient physical memory to
/// satisfy the request.
pub type extent_commit_t = extern "C" fn(
    extent_hooks: *mut extent_hooks_t,
    addr: *mut c_void,
    size: size_t,
    offset: size_t,
    length: size_t,
    arena_ind: c_uint,
) -> c_bool;

/// Extent decommit function.
///
/// Decommits any physical memory that is backing pages within an extent at
/// given `addr` and `size` at `offset` bytes, extending for `length` on behalf of arena
/// `arena_ind`, returning `false` upon success, in which case the pages will be
/// committed via the extent commit function before being reused.
///
/// If the function returns `true`, this indicates opt-out from decommit; the
/// memory remains committed and available for future use, in which case it will
/// be automatically retained for later reuse.
pub type extent_decommit_t = extern "C" fn(
    extent_hooks: *mut extent_hooks_t,
    addr: *mut c_void,
    size: size_t,
    offset: size_t,
    length: size_t,
    arena_ind: c_uint,
) -> c_bool;

/// Extent purge function.
///
/// Discards physical pages within the virtual memory mapping associated with an
/// extent at given `addr` and `size` at `offset` bytes, extending for `length` on
/// behalf of arena `arena_ind`.
///
/// A lazy extent purge function (e.g. implemented via `madvise(...MADV_FREE)`)
/// can delay purging indefinitely and leave the pages within the purged virtual
/// memory range in an indeterminite state, whereas a forced extent purge
/// function immediately purges, and the pages within the virtual memory range
/// will be zero-filled the next time they are accessed. If the function returns
/// `true`, this indicates failure to purge.
pub type extent_purge_t = extern "C" fn(
    extent_hooks: *mut extent_hooks_t,
    addr: *mut c_void,
    size: size_t,
    offset: size_t,
    length: size_t,
    arena_ind: c_uint,
) -> c_bool;

/// Extent split function.
///
/// Optionally splits an extent at given `addr` and `size` into two adjacent
/// extents, the first of `size_a` bytes, and the second of `size_b` bytes,
/// operating on `committed`/decommitted memory as indicated, on behalf of arena
/// `arena_ind`, returning `false` upon success.
///
/// If the function returns `true`, this indicates that the extent remains
/// unsplit and therefore should continue to be operated on as a whole.
pub type extent_split_t = extern "C" fn(
    extent_hooks: *mut extent_hooks_t,
    addr: *mut c_void,
    size: size_t,
    size_a: size_t,
    size_b: size_t,
    committed: c_bool,
    arena_ind: c_uint,
) -> c_bool;

/// Extent merge function.
///
/// Optionally merges adjacent extents, at given `addr_a` and `size_a` with given
/// `addr_b` and `size_b` into one contiguous extent, operating on
/// `committed`/decommitted memory as indicated, on behalf of arena `arena_ind`,
/// returning `false` upon success.
///
/// If the function returns `true`, this indicates that the extents remain
/// distinct mappings and therefore should continue to be operated on
/// independently.
pub type extent_merge_t = extern "C" fn(
    extent_hooks: *mut extent_hooks_t,
    addr_a: *mut c_void,
    size_a: size_t,
    addr_b: *mut c_void,
    size_b: size_t,
    committed: c_bool,
    arena_ind: c_uint,
) -> c_bool;

// These symbols are used by jemalloc on android but the really old android
// we're building on doesn't have them defined, so just make sure the symbols
// are available.
#[no_mangle]
#[cfg(target_os = "android")]
#[doc(hidden)]
pub extern "C" fn pthread_atfork(
    _prefork: *mut u8,
    _postfork_parent: *mut u8,
    _postfork_child: *mut u8,
) -> i32 {
    0
}
