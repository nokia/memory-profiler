use libc;

pub use common::Timestamp;

pub fn get_timestamp() -> Timestamp {
    let mut timespec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };

    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut timespec);
    }

    Timestamp::from_timespec(timespec.tv_sec as u64, timespec.tv_nsec as u64)
}

pub fn get_wall_clock() -> (Timestamp, u64, u64) {
    let timestamp = get_timestamp();
    let mut timespec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };

    unsafe {
        libc::clock_gettime(libc::CLOCK_REALTIME, &mut timespec);
    }

    (timestamp, timespec.tv_sec as u64, timespec.tv_nsec as u64)
}
