use std::mem;
use std::net::IpAddr;
use std::ptr;

use libc;

#[cfg(unix)]
pub fn get_local_ips() -> Vec<IpAddr> {
    let mut output = Vec::new();
    let mut head: *mut libc::ifaddrs = ptr::null_mut();
    unsafe {
        if libc::getifaddrs(&mut head) < 0 {
            return output;
        }
    }

    let mut p = head;
    while p != ptr::null_mut() {
        unsafe {
            if (*p).ifa_addr != ptr::null_mut() {
                let addr = &*(*p).ifa_addr;
                if addr.sa_family == libc::AF_INET as _ {
                    let addr: &libc::sockaddr_in = mem::transmute(addr);
                    let mut addr = addr.sin_addr.s_addr;
                    if cfg!(target_endian = "little") {
                        addr = addr.swap_bytes();
                    }
                    output.push(IpAddr::V4(addr.into()));
                }
            }
            p = (*p).ifa_next;
        }
    }

    unsafe {
        libc::freeifaddrs(head);
    }

    output
}

#[cfg(not(unix))]
pub fn get_local_ips() -> Vec<IpAddr> {
    Vec::new()
}
