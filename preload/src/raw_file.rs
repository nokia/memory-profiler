use libc;
use std::ffi::CStr;
use std::io::{self, Write};
use std::mem;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::path::Path;

use crate::syscall;
use crate::utils::stack_null_terminate;

#[repr(transparent)]
pub struct RawFile {
    fd: libc::c_int,
}

impl RawFile {
    pub fn borrow_raw(fd: &libc::c_int) -> &RawFile {
        unsafe { &*(fd as *const libc::c_int as *const RawFile) }
    }

    pub fn create<P: AsRef<Path>>(path: P, permissions: libc::c_int) -> Result<Self, io::Error> {
        let path = path.as_ref();

        let fd = stack_null_terminate(path.to_str().unwrap().as_bytes(), |path| {
            let path = CStr::from_bytes_with_nul(path).unwrap();
            syscall::open(
                path,
                libc::O_CLOEXEC | libc::O_CREAT | libc::O_TRUNC | libc::O_WRONLY,
                permissions,
            )
        });

        if fd < 0 {
            return Err(io::Error::from_raw_os_error(fd));
        }

        let fp = RawFile { fd };

        Ok(fp)
    }

    pub fn chmod(&self, permissions: libc::mode_t) {
        syscall::fchmod(self.fd, permissions);
    }
}

impl Drop for RawFile {
    #[inline]
    fn drop(&mut self) {
        syscall::close(self.fd);
    }
}

impl AsRawFd for RawFile {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl IntoRawFd for RawFile {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl FromRawFd for RawFile {
    #[inline]
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        RawFile { fd }
    }
}

impl Write for RawFile {
    #[inline]
    fn write(&mut self, buffer: &[u8]) -> Result<usize, io::Error> {
        <&RawFile as Write>::write(&mut &*self, buffer)
    }

    #[inline]
    fn flush(&mut self) -> Result<(), io::Error> {
        <&RawFile as Write>::flush(&mut &*self)
    }
}

impl<'a> Write for &'a RawFile {
    #[inline]
    fn write(&mut self, buffer: &[u8]) -> Result<usize, io::Error> {
        let count = syscall::write(self.fd, buffer);
        if count < 0 {
            Err(io::Error::from_raw_os_error(count as _))
        } else {
            Ok(count as _)
        }
    }

    #[inline]
    fn flush(&mut self) -> Result<(), io::Error> {
        Ok(())
    }
}

pub fn rename<S: AsRef<Path>, D: AsRef<Path>>(src: S, dst: D) -> Result<(), io::Error> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    let errcode = stack_null_terminate(src.to_str().unwrap().as_bytes(), |src| {
        let src = CStr::from_bytes_with_nul(src).unwrap();
        stack_null_terminate(dst.to_str().unwrap().as_bytes(), |dst| {
            let dst = CStr::from_bytes_with_nul(dst).unwrap();
            syscall::rename(src, dst)
        })
    });

    if errcode == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(errcode as _))
    }
}
