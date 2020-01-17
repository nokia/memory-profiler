use std::io::{self, Read};

use common::event::{Event, HeaderBody};

use crate::threaded_lz4_stream::Lz4Reader;
use common::speedy::{Endianness, Readable};

pub struct Iter<T: Read + Send> {
    fp: Lz4Reader<T>,
    done: bool,
}

impl<T> Iterator for Iter<T>
where
    T: Read + Send,
{
    type Item = io::Result<Event<'static>>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        match Event::read_from_stream(Endianness::LittleEndian, &mut self.fp) {
            Ok(event) => Some(Ok(event)),
            Err(err) => {
                self.done = true;
                if err.kind() == io::ErrorKind::UnexpectedEof {
                    None
                } else {
                    Some(Err(err))
                }
            }
        }
    }
}

pub fn parse_events<T>(
    fp: T,
) -> io::Result<(HeaderBody, impl Iterator<Item = io::Result<Event<'static>>>)>
where
    T: Read + Send + 'static,
{
    let mut fp = Lz4Reader::new(fp);

    let event = Event::read_from_stream(Endianness::LittleEndian, &mut fp)?;
    let header = match event {
        Event::Header(header) => header,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "data file doesn't start with a proper header",
            ));
        }
    };

    let iter = Iter { fp, done: false };
    Ok((header, iter))
}
