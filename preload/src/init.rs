use std::env;

use crate::global::on_exit;
use crate::logger;
use crate::opt;
use crate::utils::generate_filename;

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
    extern "C" fn sigusr_handler(signal: libc::c_int) {
        let signal_name = match signal {
            libc::SIGUSR1 => "SIGUSR1",
            libc::SIGUSR2 => "SIGUSR2",
            _ => "???",
        };

        info!(
            "Signal handler triggered with signal: {} ({})",
            signal_name, signal
        );
        crate::global::toggle();
    }

    if opt::get().register_sigusr1 {
        info!("Registering SIGUSR1 handler...");
        unsafe {
            libc::signal(libc::SIGUSR1, sigusr_handler as libc::sighandler_t);
        }
    }

    if opt::get().register_sigusr2 {
        info!("Registering SIGUSR2 handler...");
        unsafe {
            libc::signal(libc::SIGUSR2, sigusr_handler as libc::sighandler_t);
        }
    }
}

pub fn startup() {
    initialize_logger();
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    unsafe {
        opt::initialize();
    }

    initialize_atexit_hook();
    if !opt::get().disabled_by_default {
        crate::global::toggle();
    }

    initialize_signal_handlers();

    env::remove_var("LD_PRELOAD");
    info!("Startup initialization finished");
}
