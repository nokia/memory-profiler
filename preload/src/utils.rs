use std::fmt;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::syscall;
use crate::{EXECUTABLE, PID};

pub fn read_file(path: &str) -> io::Result<Vec<u8>> {
    let mut fp = File::open(path)?;
    let mut buffer = Vec::new();
    fp.read_to_end(&mut buffer)?;
    Ok(buffer)
}

pub fn copy<I: Read, O: Write>(mut input: I, mut output: O) -> io::Result<()> {
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[0..count])?;
    }
    Ok(())
}

pub struct RestoreFileCreationMaskOnDrop(libc::c_int);
impl Drop for RestoreFileCreationMaskOnDrop {
    fn drop(&mut self) {
        syscall::umask(self.0);
    }
}

pub fn temporarily_change_umask(umask: libc::c_int) -> RestoreFileCreationMaskOnDrop {
    let old_umask = syscall::umask(umask);
    RestoreFileCreationMaskOnDrop(old_umask)
}

fn stack_format<R, F, G>(format_callback: F, use_callback: G) -> R
where
    F: FnOnce(&mut &mut [u8]),
    G: FnOnce(&[u8]) -> R,
{
    let mut buffer: [u8; 1024] = unsafe { mem::uninitialized() };
    let p = {
        let mut p = &mut buffer[..];
        format_callback(&mut p);
        p.as_ptr() as usize
    };

    let length = p - buffer.as_ptr() as usize;
    use_callback(&buffer[0..length])
}

#[test]
fn test_stack_format() {
    stack_format(
        |out| {
            let _ = write!(out, "foo = {}", "bar");
            let _ = write!(out, ";");
        },
        |output| {
            assert_eq!(output, b"foo = bar;");
        },
    );
}

pub fn stack_format_bytes<R, F>(args: fmt::Arguments, callback: F) -> R
where
    F: FnOnce(&[u8]) -> R,
{
    stack_format(
        |out| {
            let _ = out.write_fmt(args);
        },
        callback,
    )
}

pub fn stack_null_terminate<R, F>(input: &[u8], callback: F) -> R
where
    F: FnOnce(&[u8]) -> R,
{
    stack_format(
        |out| {
            let _ = out.write_all(input);
            let _ = out.write_all(&[0]);
        },
        callback,
    )
}

pub fn generate_filename(pattern: &str, counter: Option<&AtomicUsize>) -> String {
    let mut output = String::new();
    let mut seen_percent = false;
    for ch in pattern.chars() {
        if !seen_percent && ch == '%' {
            seen_percent = true;
            continue;
        }

        if seen_percent {
            seen_percent = false;
            match ch {
                '%' => {
                    output.push(ch);
                }
                'p' => {
                    let pid = *PID;
                    write!(&mut output, "{}", pid).unwrap();
                }
                't' => {
                    let timestamp = unsafe { libc::time(ptr::null_mut()) };
                    write!(&mut output, "{}", timestamp).unwrap();
                }
                'e' => {
                    let executable = String::from_utf8_lossy(&*EXECUTABLE);
                    let executable =
                        &executable[executable.rfind("/").map(|index| index + 1).unwrap_or(0)..];
                    write!(&mut output, "{}", executable).unwrap();
                }
                'n' => {
                    if let Some(counter) = counter {
                        let value = counter.fetch_add(1, Ordering::SeqCst);
                        write!(&mut output, "{}", value).unwrap();
                    }
                }
                _ => {}
            }
        } else {
            output.push(ch);
        }
    }

    output
}
