use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use lz4_compress;
use std::cmp::min;
use std::io::{self, Write};

const CHUNK_SIZE: usize = 512 * 1024;

pub struct Lz4Reader<F: io::Read> {
    fp: Option<F>,
    buffer: Vec<u8>,
    compressed_buffer: Vec<u8>,
    position: usize,
}

// This doesn't matter in release mode but measurably helps in debug mode.
#[inline(always)]
fn clear(vec: &mut Vec<u8>) {
    unsafe {
        vec.set_len(0);
    }
}

impl<F: io::Read> Lz4Reader<F> {
    pub fn new(fp: F) -> Self {
        Lz4Reader {
            fp: Some(fp),
            buffer: Vec::new(),
            compressed_buffer: Vec::new(),
            position: 0,
        }
    }

    #[inline(always)]
    fn read_cached(&mut self, buf: &mut [u8]) -> usize {
        let len = min(buf.len(), self.buffer.len() - self.position);
        buf[..len].copy_from_slice(&self.buffer[self.position..self.position + len]);
        self.position += len;
        len
    }

    fn fill_cache(&mut self) -> io::Result<()> {
        let fp = self.fp.as_mut().unwrap();
        let kind = fp.read_u8()?;
        match kind {
            1 => {
                let length = fp.read_u32::<LittleEndian>()? as usize;
                self.compressed_buffer.reserve(length);
                unsafe {
                    self.compressed_buffer.set_len(length);
                }

                fp.read_exact(&mut self.compressed_buffer[..])?;
                lz4_compress::decompress_into(&self.compressed_buffer, &mut self.buffer).map_err(
                    |_| io::Error::new(io::ErrorKind::InvalidData, "decompression error"),
                )?;
                clear(&mut self.compressed_buffer);
            }
            2 => {
                let _length = fp.read_u32::<LittleEndian>()?;
                unimplemented!();
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unexpected kind"),
                ));
            }
        }

        Ok(())
    }
}

impl<F: io::Read> io::Read for Lz4Reader<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position < self.buffer.len() {
            return Ok(self.read_cached(buf));
        }

        self.position = 0;
        clear(&mut self.buffer);
        self.fill_cache()?;

        Ok(self.read_cached(buf))
    }
}

pub struct Lz4Writer<F: io::Write> {
    fp: Option<F>,
    buffer: Vec<u8>,
    compression_buffer: Vec<u8>,
    is_compressed: bool,
}

impl<F: io::Write> Lz4Writer<F> {
    pub fn new(fp: F) -> Self {
        Lz4Writer {
            fp: Some(fp),
            buffer: Vec::new(),
            compression_buffer: Vec::new(),
            is_compressed: true,
        }
    }

    pub fn replace_inner(&mut self, fp: F) -> io::Result<()> {
        self.flush()?;
        self.fp = Some(fp);
        Ok(())
    }

    pub fn inner(&self) -> &F {
        self.fp.as_ref().unwrap()
    }

    pub fn inner_mut_without_flush(&mut self) -> &mut F {
        self.fp.as_mut().unwrap()
    }

    pub fn inner_mut(&mut self) -> io::Result<&mut F> {
        self.flush()?;
        Ok(self.fp.as_mut().unwrap())
    }

    pub fn flush_and_reset_buffers(&mut self) -> io::Result<()> {
        self.flush()?;
        self.buffer = Vec::new();
        self.compression_buffer = Vec::new();

        Ok(())
    }

    pub fn into_inner(mut self) -> io::Result<F> {
        self.flush()?;
        Ok(self.fp.take().unwrap())
    }
}

fn write_compressed<T>(
    mut fp: T,
    compression_buffer: &mut Vec<u8>,
    data: &[u8],
) -> io::Result<usize>
where
    T: io::Write,
{
    clear(compression_buffer);
    compression_buffer.reserve(CHUNK_SIZE);
    for chunk in data.chunks(CHUNK_SIZE) {
        unsafe {
            compression_buffer.set_len(5);
        }

        compression_buffer[0] = 1;
        lz4_compress::compress_into(chunk, compression_buffer);

        let length = compression_buffer.len() as u32 - 5;
        LittleEndian::write_u32(&mut compression_buffer[1..5], length);
        fp.write_all(&compression_buffer)?;

        clear(compression_buffer);
    }
    Ok(data.len())
}

fn write_uncompressed<T>(mut fp: T, data: &[u8]) -> io::Result<usize>
where
    T: io::Write,
{
    fp.write_u8(2)?;
    fp.write_u32::<LittleEndian>(data.len() as u32)?;
    let result = fp.write_all(&data);
    result?;

    Ok(data.len())
}

impl<F: io::Write> Drop for Lz4Writer<F> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl<F: io::Write> io::Write for Lz4Writer<F> {
    fn write(&mut self, slice: &[u8]) -> io::Result<usize> {
        if slice.len() >= CHUNK_SIZE {
            self.flush()?;

            let mut fp = self.fp.as_mut().unwrap();
            if self.is_compressed {
                return write_compressed(&mut fp, &mut self.compression_buffer, &slice);
            } else {
                return write_uncompressed(&mut fp, &slice);
            }
        }

        let position = self.buffer.len();
        let target = self.buffer.len() + slice.len();
        self.buffer.reserve(slice.len());
        unsafe {
            self.buffer.set_len(target);
        }

        self.buffer[position..target].copy_from_slice(slice);
        if self.buffer.len() >= CHUNK_SIZE {
            self.flush()?;
        }

        Ok(slice.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut fp = self.fp.as_mut().unwrap();
        if self.is_compressed {
            write_compressed(&mut fp, &mut self.compression_buffer, &self.buffer)?;
        } else {
            write_uncompressed(&mut fp, &self.buffer)?;
        }

        clear(&mut self.buffer);
        fp.flush()
    }
}
