use std::fmt;
use std::io;
use std::mem;

use crate::streaming_channel::{self, streaming_channel};
use bytes::Bytes;

pub struct ByteSender {
    buffer: Vec<u8>,
    tx: streaming_channel::Sender<Bytes>,
}

pub fn byte_channel() -> (ByteSender, streaming_channel::Receiver<Bytes>) {
    let (tx, rx) = streaming_channel();
    let tx = ByteSender {
        buffer: Vec::new(),
        tx,
    };

    (tx, rx)
}

impl ByteSender {
    fn write_buffer(&mut self, buffer: &[u8]) -> Result<(), ()> {
        self.buffer.extend_from_slice(buffer);
        if self.buffer.len() >= 128 * 1024 {
            self.flush_buffer()?;
        }

        Ok(())
    }

    fn flush_buffer(&mut self) -> Result<(), ()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut vec = Vec::with_capacity(self.buffer.capacity());
        mem::swap(&mut vec, &mut self.buffer);
        return self.tx.send(vec.into());
    }
}

impl Drop for ByteSender {
    fn drop(&mut self) {
        let _ = self.flush_buffer();
    }
}

impl fmt::Write for ByteSender {
    #[inline]
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        self.write_buffer(s.as_bytes()).map_err(|_| fmt::Error)
    }
}

impl io::Write for ByteSender {
    #[inline]
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.write_buffer(buffer)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "write failed"))
            .map(|_| buffer.len())
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.flush_buffer()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "write failed"))
    }
}
