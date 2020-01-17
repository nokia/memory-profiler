use libc::{self, c_int, c_void, uintptr_t};
use nwind::{LocalAddressSpace, LocalAddressSpaceOptions, UnwindControl};
use parking_lot::{RwLock, RwLockWriteGuard};
use perf_event_open::{Event, EventSource, Perf};
use std::mem::{self, transmute};
use std::sync::{Arc, Weak};

use crate::global::Tls;
use crate::opt;
use crate::spin_lock::SpinLock;

type Context = *mut c_void;
type ReasonCode = c_int;
type Callback = extern "C" fn(Context, *mut c_void) -> ReasonCode;

struct CacheEntry {
    frames: *mut usize,
    capacity: usize,
    cache: Weak<Cache>,
}

impl CacheEntry {
    #[inline(always)]
    fn pack(mut frames: Vec<usize>, cache: Weak<Cache>) -> Self {
        let entry = CacheEntry {
            frames: frames.as_mut_ptr(),
            capacity: frames.capacity(),
            cache,
        };

        mem::forget(frames);
        entry
    }

    #[inline(always)]
    fn unpack(self) -> (Vec<usize>, Weak<Cache>) {
        let frames = unsafe { Vec::from_raw_parts(self.frames, 0, self.capacity) };
        (frames, self.cache)
    }
}

pub struct Cache {
    entries: SpinLock<Vec<CacheEntry>>,
}

impl Cache {
    pub fn new() -> Self {
        Cache {
            entries: SpinLock::new(Vec::new()),
        }
    }

    pub fn clear(&self) {
        let mut entries = self.entries.lock();
        let entries: &mut Vec<_> = &mut entries;
        mem::replace(entries, Vec::new());
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        let mut entries = self.entries.lock();
        for entry in entries.drain(..) {
            mem::drop(entry.unpack());
        }
    }
}

pub struct Backtrace {
    pub frames: Vec<usize>,
    pub stale_count: Option<u32>,
    cache: Weak<Cache>,
}

impl Backtrace {
    fn reserve_from_cache(&mut self, tls: &Tls) {
        let mut entries = tls.backtrace_cache.entries.lock();
        if let Some(entry) = entries.pop() {
            let (frames, cache) = entry.unpack();
            self.frames = frames;
            self.cache = cache;
        } else {
            self.cache = Arc::downgrade(&tls.backtrace_cache);
        }
    }

    pub fn new() -> Self {
        Backtrace {
            frames: Vec::new(),
            stale_count: None,
            cache: Weak::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

impl Drop for Backtrace {
    fn drop(&mut self) {
        let frames = mem::replace(&mut self.frames, Vec::new());
        let cache_weak = mem::replace(&mut self.cache, Weak::new());
        if let Some(cache) = cache_weak.upgrade() {
            let mut entries = cache.entries.lock();
            entries.push(CacheEntry::pack(frames, cache_weak));
        }
    }
}

extern "C" {
    fn _Unwind_Backtrace(callback: Callback, data: *mut c_void) -> ReasonCode;
    fn _Unwind_GetIP(context: Context) -> uintptr_t;
    fn _Unwind_VRS_Get(
        context: Context,
        regclass: _Unwind_VRS_RegClass,
        regno: u32,
        repr: _Unwind_VRS_DataRepresentation,
        valuep: *mut c_void,
    ) -> _Unwind_VRS_Result;
}

#[allow(non_camel_case_types)]
#[repr(C)]
enum _Unwind_VRS_RegClass {
    _UVRSC_CORE = 0,
    _UVRSC_VFP = 1,
    _UVRSC_WMMXD = 3,
    _UVRSC_WMMXC = 4,
}

#[allow(non_camel_case_types)]
#[repr(C)]
enum _Unwind_VRS_DataRepresentation {
    _UVRSD_UINT32 = 0,
    _UVRSD_VFPX = 1,
    _UVRSD_UINT64 = 3,
    _UVRSD_FLOAT = 4,
    _UVRSD_DOUBLE = 5,
}

#[allow(non_camel_case_types)]
#[repr(C)]
enum _Unwind_VRS_Result {
    _UVRSR_OK = 0,
    _UVRSR_NOT_IMPLEMENTED = 1,
    _UVRSR_FAILED = 2,
}

#[cfg(not(target_arch = "arm"))]
unsafe fn get_ip(context: Context) -> uintptr_t {
    _Unwind_GetIP(context)
}

#[cfg(target_arch = "arm")]
unsafe fn get_gr(context: Context, index: c_int) -> uintptr_t {
    let mut value: uintptr_t = 0;
    _Unwind_VRS_Get(
        context,
        _Unwind_VRS_RegClass::_UVRSC_CORE,
        index as u32,
        _Unwind_VRS_DataRepresentation::_UVRSD_UINT32,
        &mut value as *mut uintptr_t as *mut c_void,
    );
    value
}

#[cfg(target_arch = "arm")]
unsafe fn get_ip(context: Context) -> uintptr_t {
    get_gr(context, 15) & (!(0x1 as uintptr_t))
}

extern "C" fn on_backtrace(context: Context, data: *mut c_void) -> ReasonCode {
    unsafe {
        let out: &mut Vec<usize> = transmute(data);
        out.push(get_ip(context) as usize);
    }

    0
}

lazy_static! {
    static ref AS: RwLock<LocalAddressSpace> = {
        let opts = LocalAddressSpaceOptions::new()
            .should_load_symbols(cfg!(feature = "logging") && log_enabled!(::log::Level::Debug));

        let mut address_space = LocalAddressSpace::new_with_opts(opts).unwrap();
        address_space.use_shadow_stack(opt::get().enable_shadow_stack);
        RwLock::new(address_space)
    };
}

static mut PERF: Option<SpinLock<Perf>> = None;

pub fn prepare_to_start_unwinding() {
    static FLAG: SpinLock<bool> = SpinLock::new(false);
    let mut flag = FLAG.lock();
    if *flag {
        return;
    }
    *flag = true;

    if !opt::get().use_perf_event_open {
        return;
    }

    if unsafe { PERF.is_some() } {
        return;
    }

    let perf = Perf::build()
        .any_cpu()
        .event_source(EventSource::SwDummy)
        .open();

    match perf {
        Ok(perf) => unsafe {
            PERF = Some(SpinLock::new(perf));
        },
        Err(error) => {
            warn!("Failed to initialize perf_event_open: {}", error);
        }
    }
}

fn reload() -> parking_lot::RwLockReadGuard<'static, LocalAddressSpace> {
    let mut address_space = AS.write();
    info!("Reloading address space");
    let update = address_space.reload().unwrap();
    crate::event::send_event(crate::event::InternalEvent::AddressSpaceUpdated {
        maps: update.maps,
        new_binaries: update.new_binaries,
    });

    RwLockWriteGuard::downgrade(address_space)
}

fn reload_if_necessary_perf_event_open(
    perf: &SpinLock<Perf>,
) -> parking_lot::RwLockReadGuard<'static, LocalAddressSpace> {
    if unsafe { perf.unsafe_as_ref().are_events_pending() } {
        let mut perf = perf.lock();
        let mut reload_address_space = false;
        for event in perf.iter() {
            match event.get() {
                Event::Mmap2(ref event)
                    if event.filename != b"//anon"
                        && event.inode != 0
                        && event.protection & libc::PROT_EXEC as u32 != 0 =>
                {
                    debug!("New executable region mmaped: {:?}", event);
                    reload_address_space = true;
                }
                Event::Lost(_) => {
                    debug!("Lost events; forcing a reload");
                    reload_address_space = true;
                }
                _ => {}
            }
        }

        if reload_address_space {
            return reload();
        }
    }

    AS.read()
}

fn reload_if_necessary_dl_iterate_phdr(
    tls: &Tls,
) -> parking_lot::RwLockReadGuard<'static, LocalAddressSpace> {
    let last_state = unsafe { tls.last_dl_state() };
    let dl_state = get_dl_state();
    if *last_state != dl_state {
        *last_state = dl_state;
        return reload();
    }

    AS.read()
}

fn get_dl_state() -> (u64, u64) {
    unsafe extern "C" fn callback(
        info: *mut libc::dl_phdr_info,
        _: libc::size_t,
        data: *mut libc::c_void,
    ) -> libc::c_int {
        let out = &mut *(data as *mut (u64, u64));
        out.0 = (*info).dlpi_adds;
        out.1 = (*info).dlpi_subs;
        1
    }

    unsafe {
        let mut out: (u64, u64) = (0, 0);
        libc::dl_iterate_phdr(Some(callback), &mut out as *mut _ as *mut libc::c_void);
        out
    }
}

#[inline(never)]
pub fn grab(tls: &Tls, out: &mut Backtrace) {
    out.reserve_from_cache(tls);
    debug_assert!(out.frames.is_empty());

    if false {
        unsafe {
            _Unwind_Backtrace(on_backtrace, transmute(&mut out.frames));
        }

        return;
    }

    let address_space = unsafe {
        if let Some(ref perf) = PERF {
            reload_if_necessary_perf_event_open(perf)
        } else {
            reload_if_necessary_dl_iterate_phdr(tls)
        }
    };

    let debug_crosscheck_unwind_results =
        opt::crosscheck_unwind_results_with_libunwind() && address_space.is_shadow_stack_enabled();
    let unwind_ctx = unsafe { tls.unwind_ctx() };
    if debug_crosscheck_unwind_results || !opt::emit_partial_backtraces() {
        address_space.unwind(unwind_ctx, |address| {
            out.frames.push(address);
            UnwindControl::Continue
        });
        out.stale_count = None;
    } else {
        let stale_count = address_space.unwind_through_fresh_frames(unwind_ctx, |address| {
            out.frames.push(address);
            UnwindControl::Continue
        });
        out.stale_count = stale_count.map(|value| value as u32);
    }

    mem::drop(address_space);

    if debug_crosscheck_unwind_results {
        let mut expected: Vec<usize> = Vec::with_capacity(out.frames.len());
        unsafe {
            _Unwind_Backtrace(on_backtrace, transmute(&mut expected));
        }

        if expected.last() == Some(&0) {
            expected.pop();
        }

        if out.frames[1..] != expected[1..] {
            info!(
                "/proc/self/maps:\n{}",
                String::from_utf8_lossy(&::std::fs::read("/proc/self/maps").unwrap()).trim()
            );

            let address_space = AS.read();
            info!("Expected: ");
            for &address in &expected {
                info!("    {:?}", address_space.decode_symbol_once(address));
            }

            info!("Actual: ");
            for &address in out.frames.iter() {
                info!("    {:?}", address_space.decode_symbol_once(address));
            }

            panic!(
                "Wrong backtrace; expected: {:?}, got: {:?}",
                expected, out.frames
            );
        }
    }
}
