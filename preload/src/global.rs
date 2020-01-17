use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use nwind::LocalUnwindContext;

use crate::arc_counter::ArcCounter;
use crate::event::{send_event, InternalEvent};
use crate::spin_lock::{SpinLock, SpinLockGuard};
use crate::syscall;
use crate::thread_local::{TlsCtor, TlsPointer};
use crate::unwind::prepare_to_start_unwinding;

struct ThreadRegistry {
    pub enabled_for_new_threads: bool,
    threads: Option<HashMap<u32, *const Tls>>,
}

impl ThreadRegistry {
    fn threads(&mut self) -> &mut HashMap<u32, *const Tls> {
        self.threads.get_or_insert_with(HashMap::new)
    }
}

const STATE_UNINITIALIZED: usize = 0;
const STATE_DISABLED: usize = 1;
const STATE_STARTING: usize = 2;
const STATE_ENABLED: usize = 3;
const STATE_STOPPING: usize = 4;
const STATE_PERMANENTLY_DISABLED: usize = 5;
static STATE: AtomicUsize = AtomicUsize::new(STATE_UNINITIALIZED);

static THREAD_RUNNING: AtomicBool = AtomicBool::new(false);
static ENABLED_BY_USER: AtomicBool = AtomicBool::new(false);

static THREAD_REGISTRY: SpinLock<ThreadRegistry> = SpinLock::new(ThreadRegistry {
    enabled_for_new_threads: false,
    threads: None,
});

static PROCESSING_THREAD_HANDLE: SpinLock<Option<std::thread::JoinHandle<()>>> =
    SpinLock::new(None);

pub fn toggle() {
    if STATE.load(Ordering::SeqCst) == STATE_PERMANENTLY_DISABLED {
        return;
    }

    let value = !ENABLED_BY_USER.load(Ordering::SeqCst);
    if value {
        info!("Tracing will be toggled ON");
    } else {
        info!("Tracing will be toggled OFF");
    }

    ENABLED_BY_USER.store(value, Ordering::SeqCst);
}

pub fn enable() -> bool {
    if STATE.load(Ordering::SeqCst) == STATE_PERMANENTLY_DISABLED {
        return false;
    }

    ENABLED_BY_USER.compare_and_swap(false, true, Ordering::SeqCst) == false
}

pub fn disable() -> bool {
    if STATE.load(Ordering::SeqCst) == STATE_PERMANENTLY_DISABLED {
        return false;
    }

    ENABLED_BY_USER.compare_and_swap(true, false, Ordering::SeqCst) == true
}

fn is_busy() -> bool {
    let state = STATE.load(Ordering::SeqCst);
    if state == STATE_STARTING || state == STATE_STOPPING {
        return true;
    }

    let is_enabled = ENABLED_BY_USER.load(Ordering::SeqCst);
    let is_thread_running = THREAD_RUNNING.load(Ordering::SeqCst);
    if !is_enabled && is_thread_running && state == STATE_ENABLED {
        return true;
    }

    false
}

fn try_sync_processing_thread_destruction() {
    let mut handle = PROCESSING_THREAD_HANDLE.lock();
    let state = STATE.load(Ordering::SeqCst);
    if state == STATE_STOPPING || state == STATE_DISABLED {
        if let Some(handle) = handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn sync() {
    try_sync_processing_thread_destruction();

    while is_busy() {
        thread::sleep(std::time::Duration::from_millis(1));
    }

    try_sync_processing_thread_destruction();
}

pub extern "C" fn on_exit() {
    if STATE.load(Ordering::SeqCst) == STATE_PERMANENTLY_DISABLED {
        return;
    }

    info!("Exit hook called");

    ENABLED_BY_USER.store(false, Ordering::SeqCst);
    send_event(InternalEvent::Exit);

    let mut count = 0;
    while THREAD_RUNNING.load(Ordering::SeqCst) == true && count < 2000 {
        unsafe {
            libc::usleep(25 * 1000);
            count += 1;
        }
    }

    info!("Exit hook finished");
}

pub unsafe extern "C" fn on_fork() {
    STATE.store(STATE_PERMANENTLY_DISABLED, Ordering::SeqCst);
    ENABLED_BY_USER.store(false, Ordering::SeqCst);
    THREAD_RUNNING.store(false, Ordering::SeqCst);
    THREAD_REGISTRY.force_unlock(); // In case we were forked when the lock was held.
    {
        let tid = syscall::gettid();
        let mut registry = THREAD_REGISTRY.lock();
        registry.enabled_for_new_threads = false;
        registry.threads().retain(|&thread_id, _| thread_id == tid);
    }

    let mut tls = get_tls();
    let tls = tls.as_mut().unwrap();
    tls.set_enabled(false);
}

fn spawn_processing_thread() {
    info!("Spawning event processing thread...");

    let mut thread_handle = PROCESSING_THREAD_HANDLE.lock();
    assert!(!THREAD_RUNNING.load(Ordering::SeqCst));

    let new_handle = thread::Builder::new()
        .name("mem-prof".into())
        .spawn(move || {
            {
                let tls = unsafe { TLS.get().unwrap() };
                tls.is_internal = true;
                assert!(!tls.is_enabled());
            }
            THREAD_RUNNING.store(true, Ordering::SeqCst);

            let result = std::panic::catch_unwind(|| {
                crate::processing_thread::thread_main();
            });

            if result.is_err() {
                ENABLED_BY_USER.store(false, Ordering::SeqCst);
            }

            let mut thread_registry = THREAD_REGISTRY.lock();
            thread_registry.enabled_for_new_threads = false;
            for tls in thread_registry.threads().values() {
                let tls = unsafe { &**tls };
                if tls.is_internal {
                    continue;
                }

                debug!("Disabling thread {:04x}...", tls.thread_id);
                tls.set_enabled(false);
                tls.backtrace_cache.clear();
            }

            STATE.store(STATE_DISABLED, Ordering::SeqCst);
            info!("Tracing was disabled");

            THREAD_RUNNING.store(false, Ordering::SeqCst);

            if let Err(err) = result {
                std::panic::resume_unwind(err);
            }
        })
        .expect("failed to start the main memory profiler thread");

    while THREAD_RUNNING.load(Ordering::SeqCst) == false {
        thread::yield_now();
    }

    *thread_handle = Some(new_handle);
}

#[inline(never)]
fn try_enable(state: usize) -> bool {
    if state == STATE_UNINITIALIZED {
        STATE.store(STATE_DISABLED, Ordering::SeqCst);
        crate::init::startup();
    }

    if !ENABLED_BY_USER.load(Ordering::Relaxed) {
        return false;
    }

    if STATE.compare_and_swap(STATE_DISABLED, STATE_STARTING, Ordering::SeqCst) != STATE_DISABLED {
        return false;
    }

    static LOCK: SpinLock<()> = SpinLock::new(());
    let mut _lock = match LOCK.try_lock() {
        Some(guard) => guard,
        None => {
            return false;
        }
    };

    {
        let thread_registry = THREAD_REGISTRY.lock();
        assert!(!thread_registry.enabled_for_new_threads);
    }

    prepare_to_start_unwinding();
    spawn_processing_thread();

    {
        let mut thread_registry = THREAD_REGISTRY.lock();
        thread_registry.enabled_for_new_threads = true;
        for tls in thread_registry.threads().values() {
            let tls = unsafe { &**tls };
            if tls.is_internal {
                continue;
            }

            debug!("Enabling thread {:04x}...", tls.thread_id);
            tls.set_enabled(true);
        }
    }

    STATE.store(STATE_ENABLED, Ordering::SeqCst);
    info!("Tracing was enabled");

    true
}

pub fn try_disable_if_requested() {
    if ENABLED_BY_USER.load(Ordering::Relaxed) {
        return;
    }

    if STATE.compare_and_swap(STATE_ENABLED, STATE_STOPPING, Ordering::SeqCst) != STATE_ENABLED {
        return;
    }

    send_event(InternalEvent::Exit);
}

#[inline(always)]
pub fn acquire_lock() -> Option<(RecursionLock<'static>, ThrottleHandle)> {
    let state = STATE.load(Ordering::Relaxed);
    if state != STATE_ENABLED {
        if !try_enable(state) {
            return None;
        }
    }

    let tls = get_tls()?;
    if !tls.is_enabled() {
        None
    } else {
        let throttle = ThrottleHandle::new(&tls);
        Some((RecursionLock::new(tls), throttle))
    }
}

const THROTTLE_LIMIT: usize = 8192;

pub struct ThrottleHandle(ArcCounter);
impl ThrottleHandle {
    fn new(tls: &Tls) -> Self {
        let state = &tls.throttle_state;
        while state.get() >= THROTTLE_LIMIT {
            thread::yield_now();
        }

        ThrottleHandle(state.clone())
    }
}

pub struct AllocationLock {
    current_thread_id: u32,
    registry_lock: SpinLockGuard<'static, ThreadRegistry>,
}

impl AllocationLock {
    pub fn new() -> Self {
        let mut registry_lock = THREAD_REGISTRY.lock();
        let current_thread_id = syscall::gettid();
        let threads = registry_lock.threads();
        for (&thread_id, tls) in threads.iter_mut() {
            if thread_id == current_thread_id {
                continue;
            }

            unsafe {
                let tls = &**tls;
                if tls.is_internal {
                    continue;
                }
                tls.throttle_state.add(THROTTLE_LIMIT);
            }
        }

        std::sync::atomic::fence(Ordering::SeqCst);

        for (&thread_id, tls) in threads.iter_mut() {
            if thread_id == current_thread_id {
                continue;
            }

            unsafe {
                let tls = &**tls;
                if tls.is_internal {
                    continue;
                }
                while tls.throttle_state.get() != THROTTLE_LIMIT {
                    thread::yield_now();
                }
            }
        }

        std::sync::atomic::fence(Ordering::SeqCst);

        AllocationLock {
            current_thread_id,
            registry_lock,
        }
    }
}

impl Drop for AllocationLock {
    fn drop(&mut self) {
        for (&thread_id, tls) in self.registry_lock.threads().iter_mut() {
            if thread_id == self.current_thread_id {
                continue;
            }

            unsafe {
                let tls = &**tls;
                tls.throttle_state.sub(THROTTLE_LIMIT);
            }
        }
    }
}

pub struct RecursionLock<'a> {
    tls: &'a Tls,
}

impl<'a> RecursionLock<'a> {
    fn new(tls: &'a Tls) -> Self {
        tls.set_enabled(false);
        RecursionLock { tls }
    }
}

impl<'a> Drop for RecursionLock<'a> {
    fn drop(&mut self) {
        self.tls.set_enabled(true);
    }
}

impl<'a> Deref for RecursionLock<'a> {
    type Target = Tls;
    fn deref(&self) -> &Self::Target {
        self.tls
    }
}

pub struct Tls {
    pub thread_id: u32,
    pub is_internal: bool,
    pub enabled: AtomicBool,
    pub backtrace_cache: Arc<crate::unwind::Cache>,
    pub throttle_state: ArcCounter,
    pub unwind_ctx: UnsafeCell<LocalUnwindContext>,
    last_dl_state: UnsafeCell<(u64, u64)>,
}

impl Drop for Tls {
    fn drop(&mut self) {
        let mut registry = THREAD_REGISTRY.lock();
        self.set_enabled(false);
        registry.threads().remove(&self.thread_id);
    }
}

impl Tls {
    #[inline(always)]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn set_enabled(&self, value: bool) {
        self.enabled.store(value, Ordering::Relaxed)
    }

    pub unsafe fn unwind_ctx(&self) -> &mut LocalUnwindContext {
        &mut *self.unwind_ctx.get()
    }

    pub unsafe fn last_dl_state(&self) -> &mut (u64, u64) {
        &mut *self.last_dl_state.get()
    }
}

struct Constructor;
impl TlsCtor<Tls> for Constructor {
    fn thread_local_new<F>(self, callback: F) -> TlsPointer<Tls>
    where
        F: FnOnce(Tls) -> TlsPointer<Tls>,
    {
        let thread_id = syscall::gettid();
        let mut registry = THREAD_REGISTRY.lock();

        let tls = Tls {
            thread_id,
            is_internal: false,
            enabled: AtomicBool::new(registry.enabled_for_new_threads),
            backtrace_cache: Arc::new(crate::unwind::Cache::new()),
            throttle_state: ArcCounter::new(),
            unwind_ctx: UnsafeCell::new(LocalUnwindContext::new()),
            last_dl_state: UnsafeCell::new((0, 0)),
        };

        let tls = callback(tls);
        registry
            .threads()
            .insert(thread_id, tls.get_ptr() as *const _);

        tls
    }
}

thread_local_reentrant! {
    static TLS: Tls [Constructor];
}

#[inline]
pub fn get_tls() -> Option<&'static Tls> {
    unsafe { TLS.get().map(|tls| tls as _) }
}
