use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use lz4_compress;
use parking_lot::Mutex;
use std::cmp::min;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::mem;
use std::sync::Arc;
use std::thread;

pub struct Lz4Reader<F: io::Read + Send> {
    phantom: PhantomData<F>,
    output_rx: crossbeam_channel::Receiver<(u64, Vec<u8>)>,
    queue: Vec<(u64, Vec<u8>)>,
    counter: u64,
    buffer: Vec<u8>,
    position: usize,
    error: Arc<Mutex<Option<io::Error>>>,
}

fn read_chunk(fp: &mut impl io::Read, buffer: &mut Vec<u8>) -> Result<Vec<u8>, io::Error> {
    let kind = fp.read_u8()?;
    if kind != 1 {
        unimplemented!();
    }

    let length = fp.read_u32::<LittleEndian>()? as usize;
    buffer.reserve(length);
    unsafe {
        buffer.set_len(length);
    }

    fp.read_exact(buffer)?;
    let chunk = mem::replace(buffer, Vec::new());
    Ok(chunk)
}

impl<F: io::Read + Send + 'static> Lz4Reader<F> {
    pub fn new(mut fp: F) -> Self {
        let thread_count = 1;
        let (decompress_tx, decompress_rx) = crossbeam_channel::bounded(4);
        let (output_tx, output_rx) = crossbeam_channel::bounded(4);
        let error_arc = Arc::new(Mutex::new(None));
        let error_arc_clone = error_arc.clone();

        thread::spawn(move || {
            let mut buffer = Vec::new();
            let mut counter = 0;
            loop {
                let chunk = match read_chunk(&mut fp, &mut buffer) {
                    Ok(chunk) => chunk,
                    Err(ref error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                        break;
                    }
                    Err(error) => {
                        *error_arc_clone.lock() = Some(error);
                        break;
                    }
                };

                if decompress_tx.send((counter, chunk)).is_err() {
                    break;
                }

                counter += 1;
            }
        });

        for _ in 0..thread_count {
            let decompress_rx = decompress_rx.clone();
            let output_tx = output_tx.clone();
            thread::spawn(move || {
                while let Ok((counter, input)) = decompress_rx.recv() {
                    let mut output = Vec::new();
                    if let Ok(()) = lz4_compress::decompress_into(&input, &mut output) {
                        if output_tx.send((counter, output)).is_err() {
                            break;
                        }
                    }
                }
            });
        }

        Lz4Reader {
            phantom: PhantomData,
            output_rx,
            queue: Vec::new(),
            counter: 0,
            buffer: Vec::new(),
            position: 0,
            error: error_arc,
        }
    }
}

impl<F: io::Read + Send> Lz4Reader<F> {
    #[inline(always)]
    fn read_cached(&mut self, buf: &mut [u8]) -> usize {
        let len = min(buf.len(), self.buffer.len() - self.position);
        buf[..len].copy_from_slice(&self.buffer[self.position..self.position + len]);
        self.position += len;
        len
    }

    #[inline(never)]
    fn read_slow(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        'outer: loop {
            if self.buffer.len() - self.position > 0 {
                return Ok(self.read_cached(buf));
            }

            let index = self
                .queue
                .iter()
                .position(|(counter, _)| *counter == self.counter);
            if let Some(index) = index {
                let (_, buffer) = self.queue.swap_remove(index);
                self.buffer = buffer;
                self.position = 0;
                self.counter += 1;
                continue;
            }

            loop {
                let (counter, buffer) = match self.output_rx.recv() {
                    Ok((counter, buffer)) => (counter, buffer),
                    Err(..) => {
                        if let Some(error) = self.error.lock().take() {
                            return Err(error);
                        }

                        return Ok(0);
                    }
                };

                if counter == self.counter {
                    self.buffer = buffer;
                    self.position = 0;
                    self.counter += 1;
                    continue 'outer;
                } else {
                    self.queue.push((counter, buffer));
                }
            }
        }
    }

    #[inline(never)]
    fn read_exact_slow(&mut self, mut buf: &mut [u8]) -> io::Result<()> {
        while !buf.is_empty() {
            match io::Read::read(self, buf) {
                Ok(0) => break,
                Ok(n) => {
                    let tmp = buf;
                    buf = &mut tmp[n..];
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }

        if !buf.is_empty() {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ))
        } else {
            Ok(())
        }
    }
}

impl<F: io::Read + Send> io::Read for Lz4Reader<F> {
    #[inline(always)]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.buffer.len() - self.position > 0 {
            return Ok(self.read_cached(buf));
        }

        self.read_slow(buf)
    }

    #[inline(always)]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        if self.buffer.len() - self.position >= buf.len() {
            self.read_cached(buf);
            return Ok(());
        }

        self.read_exact_slow(buf)
    }
}

pub struct Lz4Writer<F: io::Write + Send + 'static> {
    phantom: PhantomData<F>,
    compress_tx: Option<crossbeam_channel::Sender<(u64, Vec<u8>)>>,
    thread_handle: Option<thread::JoinHandle<Result<(), io::Error>>>,
    buffer: Vec<u8>,
    counter: u64,
}

impl<F: io::Write + Send + 'static> Lz4Writer<F> {
    pub fn new(mut fp: F) -> Self {
        let thread_count = 1;
        let (compress_tx, compress_rx) = crossbeam_channel::bounded(4);
        let (merge_tx, merge_rx) = crossbeam_channel::bounded(4);

        for _ in 0..thread_count {
            let compress_rx = compress_rx.clone();
            let merge_tx = merge_tx.clone();
            thread::spawn(move || {
                while let Ok((counter, chunk)) = compress_rx.recv() {
                    let chunk: Vec<u8> = chunk;
                    let buffer = lz4_compress::compress(&chunk);
                    if merge_tx.send((counter, buffer)).is_err() {
                        break;
                    }
                }
            });
        }

        let thread_handle = thread::spawn(move || {
            let mut expected_counter = 0;
            let mut queue = Vec::new();
            'outer: while let Ok((counter, mut buffer)) = merge_rx.recv() {
                if counter != expected_counter {
                    queue.push((counter, buffer));
                    continue;
                }

                loop {
                    fp.write_u8(1)?;
                    fp.write_u32::<LittleEndian>(buffer.len() as u32)?;
                    fp.write_all(&buffer)?;

                    expected_counter += 1;

                    let index = queue
                        .iter()
                        .position(|(counter, _)| *counter == expected_counter);
                    if let Some(index) = index {
                        buffer = queue.swap_remove(index).1;
                    } else {
                        continue 'outer;
                    }
                }
            }

            let result: Result<(), io::Error> = Ok(());
            result
        });

        Lz4Writer {
            phantom: PhantomData,
            compress_tx: Some(compress_tx),
            thread_handle: Some(thread_handle),
            buffer: Vec::new(),
            counter: 0,
        }
    }

    fn join(&mut self) -> Result<(), io::Error> {
        self.flush()?;
        self.compress_tx = None;

        if let Some(handle) = self.thread_handle.take() {
            handle.join().unwrap()
        } else {
            Ok(())
        }
    }
}

impl<F: io::Write + Send + 'static> Drop for Lz4Writer<F> {
    fn drop(&mut self) {
        let _ = self.join();
    }
}

impl<F: io::Write + Send + 'static> io::Write for Lz4Writer<F> {
    #[inline(always)]
    fn write(&mut self, slice: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(slice);
        if self.buffer.len() >= 512 * 1024 {
            self.flush()?;
        }

        Ok(slice.len())
    }

    #[inline(always)]
    fn write_all(&mut self, buf: &[u8]) -> Result<(), io::Error> {
        self.write(buf)?;
        Ok(())
    }

    #[inline(never)]
    fn flush(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let next_length = self.buffer.len() + 1024 * 8;
        let buffer = mem::replace(&mut self.buffer, Vec::with_capacity(next_length));
        if self
            .compress_tx
            .as_ref()
            .unwrap()
            .send((self.counter, buffer))
            .is_err()
        {
            self.compress_tx = None;
            if let Some(handle) = self.thread_handle.take() {
                handle.join().unwrap()?;
            }
        }
        self.counter += 1;
        Ok(())
    }
}
