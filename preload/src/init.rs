use nix::sys::signal;
use std::env;
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::global::on_exit;
use crate::logger;
use crate::opt;
use crate::spin_lock::SpinLock;
use crate::utils::generate_filename;

static SIGNALWAIT_THREAD_HANDLE: SpinLock<Option<thread::JoinHandle<()>>> =
    SpinLock::new(None);

fn initialize_logger() {
    static mut SYSCALL_LOGGER: logger::SyscallLogger = logger::SyscallLogger::empty();
    static mut FILE_LOGGER: logger::FileLogger = logger::FileLogger::empty();
    let log_level = if let Ok(value) = env::var("MEMORY_PROFILER_LOG") {
        match value.as_str() {
            "trace" => log::LevelFilter::Trace,
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            "warn" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            _ => log::LevelFilter::Off,
        }
    } else {
        log::LevelFilter::Off
    };

    let pid = unsafe { libc::getpid() };

    if let Ok(value) = env::var("MEMORY_PROFILER_LOGFILE") {
        let path = generate_filename(&value, None);
        let rotate_at = env::var("MEMORY_PROFILER_LOGFILE_ROTATE_WHEN_BIGGER_THAN")
            .ok()
            .and_then(|value| value.parse().ok());

        unsafe {
            if let Ok(()) = FILE_LOGGER.initialize(path, rotate_at, log_level, pid) {
                log::set_logger(&FILE_LOGGER).unwrap();
            }
        }
    } else {
        unsafe {
            SYSCALL_LOGGER.initialize(log_level, pid);
            log::set_logger(&SYSCALL_LOGGER).unwrap();
        }
    }

    log::set_max_level(log_level);
}

fn initialize_atexit_hook() {
    info!("Setting atexit hook...");
    unsafe {
        let result = libc::atexit(on_exit);
        if result != 0 {
            error!("Cannot set the at-exit hook");
        }
    }
}

fn initialize_signal_handlers() {
    if !opt::get().register_sigusr1 && !opt::get().register_sigusr2 {
        info!("Skip signal register.");
        return;
    }
    let mut sigset = signal::SigSet::empty();
    if opt::get().register_sigusr1 {
        sigset.add(signal::Signal::SIGUSR1);
        info!("Registering SIGUSR1 handler...");
    }
    if opt::get().register_sigusr2 {
        sigset.add(signal::Signal::SIGUSR2);
        info!("Registering SIGUSR2 handler...");
    }
    sigset.thread_block().expect("Register signal failed!");

    let mut thread_handle = SIGNALWAIT_THREAD_HANDLE.lock();
    static SIG_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);
    let new_handle = thread::Builder::new()
        .name("Sigwait".into())
        .spawn(move || loop {
            match sigset.wait() {
                Ok(sig) => {
                    let signal_name = match sig {
                        signal::Signal::SIGUSR1 => "SIGUSR1",
                        signal::Signal::SIGUSR2 => "SIGUSR2",
                        _ => "???",
                    };

                    info!(
                        "Signal handler triggered with signal: {} ({})",
                        signal_name, sig
                    );
                    crate::global::toggle();
                }
                Err(e) => error!("Signal wait error: {}", e),
            }
        })
        .expect("Failed to start Sigwait thread");

    while SIG_THREAD_RUNNING.load(Ordering::SeqCst) == false {
        thread::yield_now();
    }

    *thread_handle = Some(new_handle);
}

pub fn startup() {
    initialize_logger();
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    unsafe {
        opt::initialize();
    }

    initialize_atexit_hook();

    initialize_signal_handlers();

    if !opt::get().disabled_by_default {
        crate::global::toggle();
    }

    env::remove_var("LD_PRELOAD");
    info!("Startup initialization finished");
}
