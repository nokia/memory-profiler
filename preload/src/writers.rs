use std::collections::HashSet;
use std::ffi::CStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::mem;
use std::ops::Deref;
use std::path::Path;

use nwind::proc_maps::parse as parse_maps;
use nwind::proc_maps::Region;

use common::event::{DataId, Event, FramesInvalidated, HeaderBody, HEADER_FLAG_IS_LITTLE_ENDIAN};
use common::speedy::{Endianness, Writable};
use common::Timestamp;

use crate::arch;
use crate::opt;
use crate::timestamp::{get_timestamp, get_wall_clock};
use crate::unwind::Backtrace;
use crate::utils::read_file;
use crate::{CMDLINE, EXECUTABLE, PID};

fn read_maps() -> io::Result<Vec<Region>> {
    let maps = read_file("/proc/self/maps")?;
    let maps_str = String::from_utf8_lossy(&maps);
    let regions = parse_maps(&maps_str);
    Ok(regions)
}

fn mmap_file<P: AsRef<Path>, R, F: FnOnce(&[u8]) -> R>(path: P, callback: F) -> io::Result<R> {
    let fp = File::open(&path)?;
    let mmap = unsafe { memmap::Mmap::map(&fp) }?;
    let slice = mmap.deref();
    Ok(callback(slice))
}

fn write_file<U: Write>(mut serializer: &mut U, path: &str, bytes: &[u8]) -> io::Result<()> {
    Event::File {
        timestamp: get_timestamp(),
        path: path.into(),
        contents: bytes.into(),
    }
    .write_to_stream(Endianness::LittleEndian, &mut serializer)
}

fn new_header_body(id: DataId, initial_timestamp: Timestamp) -> io::Result<HeaderBody> {
    let (timestamp, wall_clock_secs, wall_clock_nsecs) = get_wall_clock();

    let mut flags = 0;
    if arch::IS_LITTLE_ENDIAN {
        flags |= HEADER_FLAG_IS_LITTLE_ENDIAN;
    }

    Ok(HeaderBody {
        id,
        initial_timestamp,
        timestamp,
        wall_clock_secs,
        wall_clock_nsecs,
        pid: *PID,
        cmdline: CMDLINE.clone(),
        executable: EXECUTABLE.clone(),
        arch: arch::TARGET_ARCH.to_string(),
        flags,
        pointer_size: mem::size_of::<usize>() as u8,
    })
}

pub fn write_header<U: Write>(
    id: DataId,
    initial_timestamp: Timestamp,
    serializer: &mut U,
) -> io::Result<()> {
    Event::Header(new_header_body(id, initial_timestamp)?)
        .write_to_stream(Endianness::LittleEndian, serializer)
}

pub fn write_binaries<U: Write>(mut serializer: &mut U) -> io::Result<()> {
    let regions = read_maps()?;
    let mut files = HashSet::new();
    for region in regions {
        if region.is_shared || !region.is_executable || region.name.is_empty() {
            continue;
        }

        if region.name == "[heap]" || region.name == "[stack]" || region.name == "[vdso]" {
            continue;
        }

        if files.contains(&region.name) {
            continue;
        }

        files.insert(region.name);
    }

    serializer.flush()?;
    for filename in files {
        debug!("Writing '{}'...", filename);
        match mmap_file(&filename, |bytes| {
            write_file(&mut serializer, &filename, bytes)
        }) {
            Ok(result) => result?,
            Err(error) => {
                debug!("Failed to mmap '{}': {}", filename, error);
            }
        }
    }

    Ok(())
}

pub fn write_maps<U: Write>(serializer: &mut U) -> io::Result<Vec<u8>> {
    let maps = read_file("/proc/self/maps")?;
    Event::File {
        timestamp: get_timestamp(),
        path: "/proc/self/maps".into(),
        contents: maps.clone().into(),
    }
    .write_to_stream(Endianness::LittleEndian, serializer)?;
    Ok(maps)
}

fn write_wallclock<U: Write>(serializer: &mut U) -> io::Result<()> {
    let (timestamp, sec, nsec) = get_wall_clock();
    Event::WallClock {
        timestamp,
        sec,
        nsec,
    }
    .write_to_stream(Endianness::LittleEndian, serializer)
}

fn write_uptime<U: Write>(serializer: &mut U) -> io::Result<()> {
    let uptime = fs::read("/proc/uptime")?;
    write_file(serializer, "/proc/uptime", &uptime)
}

fn write_environ<U: Write>(mut serializer: U) -> io::Result<()> {
    extern "C" {
        static environ: *const *const libc::c_char;
    }

    unsafe {
        let mut ptr = environ;
        while !(*ptr).is_null() {
            let string = CStr::from_ptr(*ptr);
            Event::Environ {
                entry: string.to_bytes().into(),
            }
            .write_to_stream(Endianness::LittleEndian, &mut serializer)?;

            ptr = ptr.offset(1);
        }
    }

    Ok(())
}

pub fn write_backtrace<U: Write>(
    serializer: &mut U,
    thread: u32,
    backtrace: Backtrace,
    next_backtrace_id: &mut u64,
) -> io::Result<u64> {
    if backtrace.is_empty() {
        return Ok(0);
    }

    let id = *next_backtrace_id;
    *next_backtrace_id = id + 1;

    let frames_invalidated = match backtrace.stale_count {
        None => FramesInvalidated::All,
        Some(value) => FramesInvalidated::Some(value),
    };

    if mem::size_of::<usize>() == mem::size_of::<u32>() {
        let frames: &[usize] = backtrace.frames.as_slice();
        let frames: &[u32] =
            unsafe { std::slice::from_raw_parts(frames.as_ptr() as *const u32, frames.len()) };
        Event::PartialBacktrace32 {
            id,
            thread,
            frames_invalidated,
            addresses: frames.into(),
        }
        .write_to_stream(Endianness::LittleEndian, serializer)?;
    } else if mem::size_of::<usize>() == mem::size_of::<u64>() {
        let frames: &[usize] = backtrace.frames.as_slice();
        let frames: &[u64] =
            unsafe { std::slice::from_raw_parts(frames.as_ptr() as *const u64, frames.len()) };
        Event::PartialBacktrace {
            id,
            thread,
            frames_invalidated,
            addresses: frames.into(),
        }
        .write_to_stream(Endianness::LittleEndian, serializer)?;
    } else {
        unreachable!();
    }

    Ok(id)
}

fn write_included_files<U: Write>(serializer: &mut U) -> io::Result<()> {
    let pattern = match opt::get().include_file {
        Some(ref pattern) => pattern,
        None => return Ok(()),
    };

    info!("Will write any files matching the pattern: {:?}", pattern);
    match glob::glob(&pattern) {
        Ok(paths) => {
            let mut any = false;
            for path in paths {
                any = true;
                let path = match path {
                    Ok(path) => path,
                    Err(_) => continue,
                };

                info!("Writing file: {:?}...", path);
                match mmap_file(&path, |bytes| {
                    write_file(serializer, &path.to_string_lossy(), bytes)
                }) {
                    Ok(result) => {
                        result?;
                    }
                    Err(error) => {
                        warn!("Failed to read {:?}: {}", path, error);
                        continue;
                    }
                }

                serializer.flush()?;
            }

            if !any {
                info!("No files matched the pattern!");
            }
        }
        Err(error) => {
            error!("Glob of {:?} failed: {}", pattern, error);
        }
    }

    Ok(())
}

pub fn write_initial_data<T>(
    id: DataId,
    initial_timestamp: Timestamp,
    mut fp: T,
) -> Result<(), io::Error>
where
    T: Write,
{
    info!("Writing initial header...");
    write_header(id, initial_timestamp, &mut fp)?;

    info!("Writing wall clock...");
    write_wallclock(&mut fp)?;

    info!("Writing uptime...");
    write_uptime(&mut fp)?;
    write_included_files(&mut fp)?;

    info!("Writing environ...");
    write_environ(&mut fp)?;

    info!("Writing maps...");
    write_maps(&mut fp)?;
    fp.flush()?;

    if opt::get().write_binaries_to_output {
        info!("Writing binaries...");
        write_binaries(&mut fp)?;
    }

    info!("Flushing...");
    fp.flush()?;
    Ok(())
}
