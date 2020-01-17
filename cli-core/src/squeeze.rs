use std::cmp::{max, min};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::mem;
use std::path::Path;

use hashbrown::HashMap;

use common::speedy::{Endianness, Writable};
use common::Timestamp;

use common::event::{AllocBody, Event};

use crate::loader::Loader;
use crate::threaded_lz4_stream::Lz4Writer;

use crate::reader::parse_events;

struct Allocation {
    counter: u64,
    backtrace: u64,
    usable_size: u64,
}

struct GroupStatistics {
    first_allocation: Timestamp,
    last_allocation: Timestamp,
    free_count: u64,
    free_size: u64,
    min_size: u64,
    max_size: u64,
}

pub fn squeeze_data<F, G>(input_fp: F, output_fp: G, tmpfile_path: &Path) -> Result<(), io::Error>
where
    F: Read + Send + 'static,
    G: Write + Send + 'static,
{
    let (header, event_stream) = parse_events(input_fp)?;

    let tfp = File::create(tmpfile_path)?;
    let mut tfp = Lz4Writer::new(tfp);
    Event::Header(header).write_to_stream(Endianness::LittleEndian, &mut tfp)?;

    let (live_allocations, mut stats_by_backtrace) = {
        let mut counter = 0;

        let mut previous_backtrace_on_thread = HashMap::new();
        let mut backtrace_cache: HashMap<Vec<u64>, u64> = Default::default();
        let mut backtrace_map: lru::LruCache<u64, u64> = lru::LruCache::new(128);
        let mut stats_by_backtrace: HashMap<u64, GroupStatistics> = Default::default();
        let mut allocations: HashMap<u64, Allocation> = Default::default();
        let mut remap_backtraces = false;

        for event in event_stream {
            let mut event = event?;
            match event {
                Event::Backtrace { id, ref addresses } => {
                    let addresses = addresses.clone().into_owned();
                    let new_id = backtrace_cache.entry(addresses).or_insert(id);

                    backtrace_map.put(id, *new_id);
                    remap_backtraces = true;

                    if id != *new_id {
                        continue;
                    }
                }
                Event::PartialBacktrace {
                    id,
                    thread,
                    frames_invalidated,
                    ref mut addresses,
                } => {
                    let addresses = Loader::expand_partial_backtrace(
                        &mut previous_backtrace_on_thread,
                        thread,
                        frames_invalidated,
                        addresses.iter().cloned(),
                    );
                    mem::replace(
                        previous_backtrace_on_thread.get_mut(&thread).unwrap(),
                        addresses.clone(),
                    );

                    let new_id = backtrace_cache.entry(addresses.clone()).or_insert(id);

                    backtrace_map.put(id, *new_id);
                    remap_backtraces = true;

                    if id != *new_id {
                        continue;
                    }

                    let event = Event::Backtrace {
                        id,
                        addresses: addresses.into(),
                    };
                    event.write_to_stream(Endianness::LittleEndian, &mut tfp)?;

                    continue;
                }
                Event::PartialBacktrace32 {
                    id,
                    thread,
                    frames_invalidated,
                    ref mut addresses,
                } => {
                    let addresses = Loader::expand_partial_backtrace(
                        &mut previous_backtrace_on_thread,
                        thread,
                        frames_invalidated,
                        addresses.iter().map(|&address| address as u64),
                    );
                    mem::replace(
                        previous_backtrace_on_thread.get_mut(&thread).unwrap(),
                        addresses.clone(),
                    );

                    let new_id = backtrace_cache.entry(addresses.clone()).or_insert(id);

                    backtrace_map.put(id, *new_id);
                    remap_backtraces = true;

                    if id != *new_id {
                        continue;
                    }

                    let event = Event::Backtrace {
                        id,
                        addresses: addresses.into(),
                    };
                    event.write_to_stream(Endianness::LittleEndian, &mut tfp)?;

                    continue;
                }
                Event::Alloc {
                    allocation:
                        AllocBody {
                            ref mut backtrace,
                            pointer,
                            size,
                            extra_usable_space,
                            ..
                        },
                    timestamp,
                    ..
                } => {
                    let usable_size = size + extra_usable_space as u64;
                    {
                        if remap_backtraces {
                            *backtrace = backtrace_map.get(backtrace).cloned().unwrap();
                        }

                        let stats = stats_by_backtrace.entry(*backtrace).or_insert_with(|| {
                            GroupStatistics {
                                first_allocation: timestamp,
                                last_allocation: timestamp,
                                free_count: 0,
                                free_size: 0,
                                min_size: usable_size,
                                max_size: usable_size,
                            }
                        });

                        stats.first_allocation = min(stats.first_allocation, timestamp);
                        stats.last_allocation = max(stats.last_allocation, timestamp);
                        stats.free_size += usable_size;
                        stats.min_size = min(stats.min_size, usable_size);
                        stats.max_size = min(stats.max_size, usable_size);
                    }

                    allocations.insert(
                        pointer,
                        Allocation {
                            counter,
                            backtrace: *backtrace,
                            usable_size,
                        },
                    );
                    counter += 1;
                }
                Event::Realloc {
                    timestamp,
                    mut allocation,
                    old_pointer,
                    ..
                } => {
                    let usable_size = allocation.size + allocation.extra_usable_space as u64;
                    {
                        if remap_backtraces {
                            allocation.backtrace =
                                backtrace_map.get(&allocation.backtrace).cloned().unwrap();
                        }

                        let stats = stats_by_backtrace
                            .entry(allocation.backtrace)
                            .or_insert_with(|| GroupStatistics {
                                first_allocation: timestamp,
                                last_allocation: timestamp,
                                free_count: 0,
                                free_size: 0,
                                min_size: usable_size,
                                max_size: usable_size,
                            });

                        stats.first_allocation = min(stats.first_allocation, timestamp);
                        stats.last_allocation = max(stats.last_allocation, timestamp);
                        stats.free_size += usable_size;
                        stats.min_size = min(stats.min_size, usable_size);
                        stats.max_size = min(stats.max_size, usable_size);
                    }

                    if let Some(old_allocation) = allocations.remove(&old_pointer) {
                        if let Some(stats) = stats_by_backtrace.get_mut(&old_allocation.backtrace) {
                            stats.free_count += 1;
                        }
                    }

                    allocations.insert(
                        allocation.pointer,
                        Allocation {
                            counter,
                            backtrace: allocation.backtrace,
                            usable_size,
                        },
                    );

                    let event = Event::Alloc {
                        timestamp,
                        allocation: allocation.clone(),
                    };
                    event.write_to_stream(Endianness::LittleEndian, &mut tfp)?;

                    counter += 1;
                    continue;
                }
                Event::Free { pointer, .. } => {
                    if let Some(allocation) = allocations.remove(&pointer) {
                        if let Some(stats) = stats_by_backtrace.get_mut(&allocation.backtrace) {
                            stats.free_count += 1;
                        }
                    }

                    continue;
                }
                Event::MemoryMap {
                    ref mut backtrace, ..
                }
                | Event::MemoryUnmap {
                    ref mut backtrace, ..
                }
                | Event::Mallopt {
                    ref mut backtrace, ..
                } => {
                    if remap_backtraces {
                        *backtrace = backtrace_map.get(backtrace).cloned().unwrap();
                    }
                }

                Event::GroupStatistics {
                    ref mut backtrace,
                    first_allocation,
                    last_allocation,
                    free_count,
                    free_size,
                    min_size,
                    max_size,
                } => {
                    {
                        if remap_backtraces {
                            *backtrace = backtrace_map.get(backtrace).cloned().unwrap();
                        }
                        let stats = stats_by_backtrace.entry(*backtrace).or_insert_with(|| {
                            GroupStatistics {
                                first_allocation,
                                last_allocation,
                                free_count: 0,
                                free_size: 0,
                                min_size,
                                max_size,
                            }
                        });

                        stats.first_allocation = min(stats.first_allocation, first_allocation);
                        stats.last_allocation = max(stats.last_allocation, last_allocation);
                        stats.min_size = min(stats.min_size, min_size);
                        stats.max_size = max(stats.max_size, max_size);
                        stats.free_count += free_count;
                        stats.free_size += free_size;
                    }

                    continue;
                }

                Event::File { .. } => {}
                Event::Header { .. } => {}
                Event::MemoryDump { .. } => {}
                Event::Marker { .. } => {}
                Event::Environ { .. } => {}
                Event::WallClock { .. } => {}
                Event::String { .. } => {}
                Event::DecodedFrame { .. } => {}
                Event::DecodedBacktrace { .. } => {}
            }

            event.write_to_stream(Endianness::LittleEndian, &mut tfp)?;
        }

        let live_allocations: HashMap<_, _> = allocations
            .into_iter()
            .map(|(pointer, allocation)| {
                stats_by_backtrace
                    .get_mut(&allocation.backtrace)
                    .unwrap()
                    .free_size -= allocation.usable_size;
                (pointer, allocation.counter)
            })
            .collect();
        (live_allocations, stats_by_backtrace)
    };

    tfp.flush()?;
    mem::drop(tfp);

    let ifp = File::open(tmpfile_path)?;
    let (header, event_stream) = parse_events(ifp)?;
    let mut ofp = Lz4Writer::new(output_fp);
    Event::Header(header).write_to_stream(Endianness::LittleEndian, &mut ofp)?;

    {
        let mut counter = 0;
        let mut last_decoded_backtrace_id = 0;
        for event in event_stream {
            let event = event?;
            let mut backtrace_id = None;
            match event {
                Event::Backtrace { id, .. } | Event::PartialBacktrace { id, .. } => {
                    backtrace_id = Some(id);
                }
                Event::DecodedBacktrace { .. } => {
                    backtrace_id = Some(last_decoded_backtrace_id);
                    last_decoded_backtrace_id += 1;
                }
                Event::Alloc {
                    allocation: AllocBody { pointer, .. },
                    ..
                } => match live_allocations.get(&pointer) {
                    Some(&last_counter) if counter == last_counter => {
                        counter += 1;
                    }
                    _ => {
                        counter += 1;
                        continue;
                    }
                },
                Event::Realloc { .. } => {
                    unreachable!();
                }
                Event::Free { .. } => {
                    unreachable!();
                }
                _ => {}
            }

            event.write_to_stream(Endianness::LittleEndian, &mut ofp)?;

            if let Some(id) = backtrace_id {
                if let Some(stats) = stats_by_backtrace.remove(&id) {
                    let event = Event::GroupStatistics {
                        backtrace: id,
                        first_allocation: stats.first_allocation,
                        last_allocation: stats.last_allocation,
                        free_count: stats.free_count,
                        free_size: stats.free_size,
                        min_size: stats.min_size,
                        max_size: stats.max_size,
                    };
                    event.write_to_stream(Endianness::LittleEndian, &mut ofp)?;
                }
            }
        }
    }

    ofp.flush()?;
    mem::drop(ofp);

    assert!(stats_by_backtrace.is_empty());
    let _ = fs::remove_file(tmpfile_path);

    Ok(())
}
