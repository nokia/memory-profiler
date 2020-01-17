use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::u64;

use hashbrown::HashSet;
use string_interner::Symbol;

use common::speedy::{Endianness, Writable};
use nwind::DebugInfoIndex;

use common::event::{AllocBody, Event};

use common::lz4_stream::Lz4Writer;

use crate::loader::Loader;
use crate::reader::parse_events;

pub fn postprocess<F, G, D, I>(ifp: F, ofp: G, debug_symbols: I) -> Result<(), io::Error>
where
    F: Read + Send + 'static,
    G: Write,
    D: AsRef<OsStr>,
    I: IntoIterator<Item = D>,
{
    let mut ofp = Lz4Writer::new(ofp);
    let (header, event_stream) = parse_events(ifp)?;

    let mut debug_info_index = DebugInfoIndex::new();
    for path in debug_symbols {
        debug_info_index.add(path.as_ref());
    }

    let mut loader = Loader::new(header.clone(), debug_info_index);
    Event::Header(header).write_to_stream(Endianness::LittleEndian, &mut ofp)?;

    let mut frames = Vec::new();
    let mut frames_to_write = Vec::new();
    let mut emitted_strings = HashSet::new();
    let mut expected_backtrace_id = 0;
    let mut expected_frame_id = 0;
    for event in event_stream {
        let mut event = event?;
        let mut process = false;
        let mut is_backtrace = false;
        let mut write = true;
        match event {
            Event::Backtrace { .. } => {
                is_backtrace = true;
                write = false;
            }
            Event::PartialBacktrace { .. } | Event::PartialBacktrace32 { .. } => {
                is_backtrace = true;
                write = false;
            }
            Event::Alloc {
                allocation: AllocBody {
                    ref mut backtrace, ..
                },
                ..
            }
            | Event::Realloc {
                allocation: AllocBody {
                    ref mut backtrace, ..
                },
                ..
            }
            | Event::Free {
                ref mut backtrace, ..
            }
            | Event::MemoryMap {
                ref mut backtrace, ..
            }
            | Event::MemoryUnmap {
                ref mut backtrace, ..
            }
            | Event::Mallopt {
                ref mut backtrace, ..
            }
            | Event::GroupStatistics {
                ref mut backtrace, ..
            } => {
                if let Some(target_backtrace) = loader.lookup_backtrace(*backtrace) {
                    *backtrace = target_backtrace.raw() as _;
                } else {
                    *backtrace = u64::MAX;
                }
            }

            Event::File {
                ref mut contents, ..
            } if contents.starts_with(b"\x7FELF") => {
                process = true;
                write = false;
            }

            Event::File { .. } => {
                process = true;
            }
            Event::Header { .. } => {}
            Event::MemoryDump { .. } => {}
            Event::Marker { .. } => {}
            Event::Environ { .. } => {}
            Event::WallClock { .. } => {}
            Event::String { .. } => {}
            Event::DecodedFrame { .. } => {}
            Event::DecodedBacktrace { .. } => {}
        }

        if write {
            event.write_to_stream(Endianness::LittleEndian, &mut ofp)?;
        }

        if is_backtrace {
            frames.clear();
            frames_to_write.clear();

            let backtrace_id = loader.process_backtrace_event(event, |frame_id, is_new| {
                frames.push(frame_id as u32);
                if is_new {
                    frames_to_write.push(frame_id);
                }
            });

            if backtrace_id.is_none() {
                assert!(frames.is_empty());
                assert!(frames_to_write.is_empty());
            }

            for frame_id in frames_to_write.drain(..) {
                let frame = loader.get_frame(frame_id).clone();
                macro_rules! intern {
                    ($value:expr) => {
                        if let Some(id) = $value {
                            let raw_id = id.to_usize() as u32;
                            if !emitted_strings.contains(&id) {
                                emitted_strings.insert(id);
                                let string = loader.interner().resolve(id).unwrap();
                                Event::String {
                                    id: raw_id,
                                    string: string.into(),
                                }
                                .write_to_stream(Endianness::LittleEndian, &mut ofp)?;
                            }

                            raw_id
                        } else {
                            0xFFFFFFFF
                        }
                    };
                }

                let library = intern!(frame.library());
                let raw_function = intern!(frame.raw_function());
                let function = intern!(frame.function());
                let source = intern!(frame.source());

                assert_eq!(frame_id, expected_frame_id);
                expected_frame_id += 1;

                Event::DecodedFrame {
                    address: frame.address().raw(),
                    library,
                    raw_function,
                    function,
                    source,
                    line: frame.line().unwrap_or(0xFFFFFFFF),
                    column: frame.column().unwrap_or(0xFFFFFFFF),
                    is_inline: frame.is_inline(),
                }
                .write_to_stream(Endianness::LittleEndian, &mut ofp)?;
            }

            if let Some(backtrace_id) = backtrace_id {
                assert_eq!(backtrace_id.raw(), expected_backtrace_id);
                expected_backtrace_id += 1;

                Event::DecodedBacktrace {
                    frames: (&frames).into(),
                }
                .write_to_stream(Endianness::LittleEndian, &mut ofp)?;
            }
        } else if process {
            loader.process(event);
        }
    }

    Ok(())
}
