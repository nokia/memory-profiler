use std::borrow::Cow;
use std::fmt;
use std::io;
use std::str::FromStr;

use speedy::{Context, Endianness, Readable, Reader, Writable, Writer};

use crate::timestamp::Timestamp;

pub const HEADER_FLAG_IS_LITTLE_ENDIAN: u64 = 1;

#[derive(Clone, PartialEq, Debug, Readable, Writable)]
pub struct HeaderBody {
    pub id: DataId,
    pub initial_timestamp: Timestamp,
    pub timestamp: Timestamp,
    pub wall_clock_secs: u64,
    pub wall_clock_nsecs: u64,
    pub pid: u32,
    pub cmdline: Vec<u8>,
    pub executable: Vec<u8>,
    pub arch: String,
    pub flags: u64,
    pub pointer_size: u8,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Readable, Writable)]
pub struct DataId(u64, u64);

impl DataId {
    pub fn new(a: u64, b: u64) -> Self {
        DataId(a, b)
    }
}

impl fmt::Display for DataId {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{:016x}{:016x}", self.0, self.1)
    }
}

impl FromStr for DataId {
    type Err = Box<dyn std::error::Error>;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        if string.len() != 32 {
            return Err("invalid ID".into());
        }

        let id_a: u64 = u64::from_str_radix(&string[0..16], 16)?;
        let id_b: u64 = u64::from_str_radix(&string[16..], 16)?;

        Ok(DataId(id_a, id_b))
    }
}

#[test]
fn test_data_id_string_conversions() {
    let id_before = DataId(0x12345678_ABCD3210, 0x9AAAAAAB_CDDDDDDF);
    assert_eq!(id_before.to_string(), "12345678abcd32109aaaaaabcddddddf");

    let id_after: DataId = id_before.to_string().parse().unwrap();
    assert_eq!(id_before, id_after);
}

pub const ALLOC_FLAG_CALLOC: u32 = 1 << 31;

// These are the same as glibc's allocator flags.
pub const ALLOC_FLAG_PREV_IN_USE: u32 = 1;
pub const ALLOC_FLAG_MMAPED: u32 = 2;
pub const ALLOC_FLAG_NON_MAIN_ARENA: u32 = 4;

#[derive(Clone, PartialEq, Debug, Readable, Writable)]
pub struct AllocBody {
    pub pointer: u64,
    pub size: u64,
    pub backtrace: u64,
    pub thread: u32,
    pub flags: u32,
    pub extra_usable_space: u32,
    pub preceding_free_space: u64,
}

#[derive(Clone, PartialEq, Debug, Readable, Writable)]
pub enum Event<'a> {
    Header(HeaderBody),
    Alloc {
        timestamp: Timestamp,
        allocation: AllocBody,
    },
    Realloc {
        timestamp: Timestamp,
        old_pointer: u64,
        allocation: AllocBody,
    },
    Free {
        timestamp: Timestamp,
        pointer: u64,
        backtrace: u64,
        thread: u32,
    },
    File {
        timestamp: Timestamp,
        path: Cow<'a, str>,
        contents: Cow<'a, [u8]>,
    },
    Backtrace {
        id: u64,
        addresses: Cow<'a, [u64]>,
    },
    MemoryDump {
        address: u64,
        length: u64,
        data: Cow<'a, [u8]>,
    },
    Marker {
        value: u32,
    },
    MemoryMap {
        timestamp: Timestamp,
        pointer: u64,
        length: u64,
        backtrace: u64,
        requested_address: u64,
        mmap_protection: u32,
        mmap_flags: u32,
        file_descriptor: u32,
        thread: u32,
        offset: u64,
    },
    MemoryUnmap {
        timestamp: Timestamp,
        pointer: u64,
        length: u64,
        backtrace: u64,
        thread: u32,
    },
    Mallopt {
        timestamp: Timestamp,
        backtrace: u64,
        thread: u32,
        param: i32,
        value: i32,
        result: i32,
    },
    Environ {
        entry: Cow<'a, [u8]>,
    },
    WallClock {
        timestamp: Timestamp,
        sec: u64,
        nsec: u64,
    },
    PartialBacktrace {
        id: u64,
        thread: u32,
        frames_invalidated: FramesInvalidated,
        addresses: Cow<'a, [u64]>,
    },
    String {
        id: u32,
        string: Cow<'a, str>,
    },
    DecodedFrame {
        address: u64,
        library: u32,
        raw_function: u32,
        function: u32,
        source: u32,
        line: u32,
        column: u32,
        is_inline: bool,
    },
    DecodedBacktrace {
        frames: Cow<'a, [u32]>,
    },
    GroupStatistics {
        backtrace: u64,
        first_allocation: Timestamp,
        last_allocation: Timestamp,
        free_count: u64,
        free_size: u64,
        min_size: u64,
        max_size: u64,
    },
    PartialBacktrace32 {
        id: u64,
        thread: u32,
        frames_invalidated: FramesInvalidated,
        addresses: Cow<'a, [u32]>,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum FramesInvalidated {
    All,
    Some(u32),
}

impl<'a, C: Context> Readable<'a, C> for FramesInvalidated {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> io::Result<Self> {
        let frames = reader.read_u32()?;
        if frames == 0xFFFFFFFF {
            Ok(FramesInvalidated::All)
        } else {
            Ok(FramesInvalidated::Some(frames))
        }
    }
}

impl<C: Context> Writable<C> for FramesInvalidated {
    fn write_to<'this, T: ?Sized + Writer<'this, C>>(
        &'this self,
        writer: &mut T,
    ) -> io::Result<()> {
        let value = match *self {
            FramesInvalidated::All => 0xFFFFFFFF,
            FramesInvalidated::Some(value) => value,
        };

        writer.write_u32(value)
    }
}

#[derive(Debug)]
pub enum FramedEvent<'a> {
    Known(Event<'a>),
    Unknown(Cow<'a, [u8]>),
}

impl<'a, C: Context> Readable<'a, C> for FramedEvent<'a> {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> io::Result<Self> {
        let length = reader.read_u32()? as usize;
        let bytes = reader.read_bytes_cow(length)?;
        match bytes {
            Cow::Borrowed(bytes) => {
                match Event::read_from_buffer(Endianness::LittleEndian, &bytes) {
                    Ok(event) => Ok(FramedEvent::Known(event)),
                    Err(_) => Ok(FramedEvent::Unknown(Cow::Borrowed(bytes))),
                }
            }
            Cow::Owned(bytes) => {
                match Event::read_from_buffer_owned(Endianness::LittleEndian, &bytes) {
                    Ok(event) => Ok(FramedEvent::Known(event)),
                    Err(_) => Ok(FramedEvent::Unknown(Cow::Owned(bytes))),
                }
            }
        }
    }
}

impl<'a, C: Context> Writable<C> for FramedEvent<'a> {
    fn write_to<'this, T: ?Sized + Writer<'this, C>>(
        &'this self,
        writer: &mut T,
    ) -> io::Result<()> {
        match self {
            &FramedEvent::Known(ref event) => {
                let length = Writable::<C>::bytes_needed(event) as u32;
                writer.write_u32(length)?;
                writer.write_value(event)?;

                Ok(())
            }
            &FramedEvent::Unknown(ref bytes) => {
                let length = bytes.len() as u32;
                writer.write_u32(length)?;
                writer.write_bytes(&bytes)?;

                Ok(())
            }
        }
    }
}
