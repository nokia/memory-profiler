use libc;
use log::{self, Level, Metadata, Record};
use std::io::{self, Write};
use std::mem;
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::raw_file::{rename, RawFile};
use crate::spin_lock::SpinLock;
use crate::syscall;
use crate::utils::{stack_format_bytes, temporarily_change_umask};

fn level_to_str(level: Level) -> &'static str {
    match level {
        Level::Error => "ERR",
        Level::Warn => "WRN",
        Level::Info => "INF",
        Level::Debug => "DBG",
        Level::Trace => "TRC",
    }
}

pub struct SyscallLogger {
    level: log::LevelFilter,
    pid: libc::pid_t,
}

impl SyscallLogger {
    pub const fn empty() -> Self {
        SyscallLogger {
            level: log::LevelFilter::Off,
            pid: 0,
        }
    }

    pub fn initialize(&mut self, level: log::LevelFilter, pid: libc::pid_t) {
        self.level = level;
        self.pid = pid;
    }
}

fn filter(record: &Record) -> bool {
    if let Some(module) = record.module_path() {
        if module.starts_with("goblin::") {
            return false;
        }
    }

    true
}

fn raw_eprint(buffer: &[u8]) {
    syscall::write(2, buffer);
}

impl log::Log for SyscallLogger {
    #[inline]
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    #[inline]
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            if !filter(record) {
                return;
            }

            stack_format_bytes(
                format_args!(
                    "memory-profiler: {:04x} {:04x} {} {}\n",
                    self.pid,
                    syscall::gettid(),
                    level_to_str(record.level()),
                    record.args()
                ),
                |buffer| {
                    raw_eprint(buffer);
                },
            );
        }
    }

    #[inline]
    fn flush(&self) {}
}

struct RotationState {
    path: String,
    old_path: String,
    initial_path: String,
    rotated: bool,
}

impl RotationState {
    fn rotate(&mut self) -> RawFile {
        let path = &self.path;
        let old_path = if !self.rotated {
            self.rotated = true;
            &self.initial_path
        } else {
            &self.old_path
        };

        if let Err(_) = rename(path, old_path) {
            raw_eprint(b"memory-profiler: Failed to rotate the log file!\n");
        }

        let fp = {
            let _handle = temporarily_change_umask(0o777);
            RawFile::create(&path, 0o777).expect("failed to recreate the logfile after rotation")
        };

        fp.chmod(0o777);
        fp
    }
}

pub struct FileLoggerOutput {
    raw_fd: AtomicUsize,
    rotation: SpinLock<RotationState>,
    bytes_written: AtomicUsize,
    rotate_at: Option<usize>,
}

impl FileLoggerOutput {
    fn new(path: String, mut rotate_at: Option<usize>) -> Result<Self, io::Error> {
        let fp = {
            let _handle = temporarily_change_umask(0o777);
            RawFile::create(&path, 0o777)?
        };

        fp.chmod(0o777);

        if rotate_at == Some(0) {
            rotate_at = None;
        }

        let initial_path = format!("{}.initial", path);
        let old_path = format!("{}.old", path);

        let output = FileLoggerOutput {
            raw_fd: AtomicUsize::new(fp.into_raw_fd() as _),
            rotation: SpinLock::new(RotationState {
                path,
                old_path,
                initial_path,
                rotated: false,
            }),
            bytes_written: AtomicUsize::new(0),
            rotate_at,
        };

        Ok(output)
    }

    fn fd(&self) -> libc::c_int {
        self.raw_fd.load(Ordering::SeqCst) as libc::c_int
    }

    fn rotate_if_necessary(&self) -> Result<(), io::Error> {
        let threshold = match self.rotate_at {
            Some(threshold) => threshold,
            None => return Ok(()),
        };

        if self.bytes_written.load(Ordering::Relaxed) < threshold {
            return Ok(());
        }

        let mut rotation = match self.rotation.try_lock() {
            Some(rotation) => rotation,
            None => return Ok(()),
        };

        if self.bytes_written.load(Ordering::SeqCst) < threshold {
            return Ok(());
        }

        let new_fp = rotation.rotate();
        let new_fd = new_fp.into_raw_fd();
        let old_fd = self.raw_fd.swap(new_fd as _, Ordering::SeqCst) as _;
        self.bytes_written.store(0, Ordering::SeqCst);

        mem::drop(unsafe { RawFile::from_raw_fd(old_fd) });

        Ok(())
    }
}

pub struct FileLogger {
    level: log::LevelFilter,
    pid: libc::pid_t,
    output: Option<FileLoggerOutput>,
}

impl FileLogger {
    pub const fn empty() -> Self {
        FileLogger {
            level: log::LevelFilter::Off,
            pid: 0,
            output: None,
        }
    }

    pub fn initialize(
        &mut self,
        path: String,
        rotate_at: Option<usize>,
        level: log::LevelFilter,
        pid: libc::pid_t,
    ) -> io::Result<()> {
        let output = FileLoggerOutput::new(path, rotate_at)?;
        self.level = level;
        self.pid = pid;
        self.output = Some(output);
        Ok(())
    }
}

impl log::Log for FileLogger {
    #[inline]
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    #[inline]
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            if !filter(record) {
                return;
            }

            if let Some(output) = self.output.as_ref() {
                stack_format_bytes(
                    format_args!(
                        "{:04x} {:04x} {} {}\n",
                        self.pid,
                        syscall::gettid(),
                        level_to_str(record.level()),
                        record.args()
                    ),
                    |buffer| {
                        let fd = output.fd();
                        let mut fp = RawFile::borrow_raw(&fd);
                        let _ = fp.write_all(buffer);
                        output
                            .bytes_written
                            .fetch_add(buffer.len(), Ordering::Relaxed);
                    },
                );
                let _ = output.rotate_if_necessary();
            }
        }
    }

    #[inline]
    fn flush(&self) {}
}
