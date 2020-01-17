use std::fmt;
use std::io;
use std::str;

pub struct IoAdapter<T>(T);

impl<T> IoAdapter<T> {
    #[inline]
    pub fn new(fp: T) -> Self {
        IoAdapter(fp)
    }
}

impl<T: io::Write> fmt::Write for IoAdapter<T> {
    #[inline]
    fn write_str(&mut self, string: &str) -> Result<(), fmt::Error> {
        self.0.write_all(string.as_bytes()).map_err(|_| fmt::Error)
    }
}

impl<T: fmt::Write> io::Write for IoAdapter<T> {
    #[inline]
    fn write(&mut self, buffer: &[u8]) -> Result<usize, io::Error> {
        let string = str::from_utf8(buffer)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.0
            .write_str(string)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "formatting error"))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        Ok(())
    }

    fn write_fmt(&mut self, args: fmt::Arguments) -> Result<(), io::Error> {
        self.0
            .write_fmt(args)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "formatting error"))
    }
}
