use std::sync::Arc;
use std::time::Duration;

use common::Timestamp;

use crate::channel::Channel;
use crate::global::ThrottleHandle;
use crate::unwind::Backtrace;

pub(crate) enum InternalEvent {
    Alloc {
        ptr: usize,
        size: usize,
        backtrace: Backtrace,
        thread: u32,
        flags: u32,
        extra_usable_space: u32,
        preceding_free_space: u64,
        timestamp: Timestamp,
        throttle: ThrottleHandle,
    },
    Realloc {
        old_ptr: usize,
        new_ptr: usize,
        size: usize,
        backtrace: Backtrace,
        thread: u32,
        flags: u32,
        extra_usable_space: u32,
        preceding_free_space: u64,
        timestamp: Timestamp,
        throttle: ThrottleHandle,
    },
    Free {
        ptr: usize,
        backtrace: Backtrace,
        thread: u32,
        timestamp: Timestamp,
        throttle: ThrottleHandle,
    },
    Exit,
    GrabMemoryDump,
    SetMarker {
        value: u32,
    },
    Mmap {
        pointer: usize,
        requested_address: usize,
        length: usize,
        mmap_protection: u32,
        mmap_flags: u32,
        offset: u64,
        backtrace: Backtrace,
        thread: u32,
        file_descriptor: u32,
        timestamp: Timestamp,
        throttle: ThrottleHandle,
    },
    Munmap {
        ptr: usize,
        len: usize,
        backtrace: Backtrace,
        thread: u32,
        timestamp: Timestamp,
        throttle: ThrottleHandle,
    },
    Mallopt {
        param: i32,
        value: i32,
        result: i32,
        backtrace: Backtrace,
        thread: u32,
        timestamp: Timestamp,
        throttle: ThrottleHandle,
    },
    OverrideNextTimestamp {
        timestamp: Timestamp,
    },
    AddressSpaceUpdated {
        maps: String,
        new_binaries: Vec<Arc<nwind::BinaryData>>,
    },
}

lazy_static! {
    static ref EVENT_CHANNEL: Channel<InternalEvent> = Channel::new();
}

pub(crate) fn send_event(event: InternalEvent) {
    EVENT_CHANNEL.send(event);
}

#[inline(always)]
pub(crate) fn send_event_throttled<F: FnOnce() -> InternalEvent>(callback: F) {
    EVENT_CHANNEL.chunked_send_with(64, callback);
}

pub(crate) fn timed_recv_all_events(output: &mut Vec<InternalEvent>, duration: Duration) {
    EVENT_CHANNEL.timed_recv_all(output, duration)
}

pub(crate) fn flush() {
    EVENT_CHANNEL.flush();
}
