use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::mem;
use std::ops::{Deref, Range};
use std::sync::Arc;
use std::time::Instant;

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use hashbrown::{hash_map, HashMap, HashSet};
use nwind::proc_maps::parse as parse_maps;
use nwind::proc_maps::Region;
use nwind::{arch, AddressSpace, BinaryData, DebugInfoIndex, IAddressSpace};

use common::event::{
    self, AllocBody, Event, FramesInvalidated, HeaderBody, HEADER_FLAG_IS_LITTLE_ENDIAN,
};
use common::range_map::RangeMap;

use crate::data::{
    Allocation, AllocationFlags, AllocationId, BacktraceId, BacktraceStorageRef, CodePointer, Data,
    DataId, DataPointer, Deallocation, FrameId, GroupStatistics, Mallopt, MapFlags, MemoryMap,
    MemoryUnmap, MmapOperation, OperationId, ProtectionFlags, StringId, StringInterner, ThreadId,
    Timestamp,
};
use crate::frame::Frame;
use crate::reader::parse_events;
use crate::vecvec::DenseVecVec;

#[derive(Clone, PartialEq, Eq, Default, Debug, Hash)]
pub struct AddressMapping {
    pub declared_address: u64,
    pub actual_address: u64,
    pub file_offset: u64,
    pub size: u64,
}

fn clean_symbol(input: Cow<str>) -> Cow<str> {
    // TODO: Make this faster.
    input
        .replace("> >", ">>")
        .replace(
            "std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char>>",
            "std::string",
        )
        .into()
}

#[test]
fn test_clean_symbol() {
    let symbol = "google::protobuf::RepeatedPtrField<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >::TypeHandler*::Type google::protobuf::internal::RepeatedPtrFieldBase::Add<google::protobuf::RepeatedPtrField<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >::TypeHandler>()";
    assert_eq!(
        clean_symbol( symbol.into() ),
        "google::protobuf::RepeatedPtrField<std::string::TypeHandler*::Type google::protobuf::internal::RepeatedPtrFieldBase::Add<google::protobuf::RepeatedPtrField<std::string::TypeHandler>()"
    );
}

pub struct Loader {
    id: DataId,
    header: HeaderBody,
    interner: RefCell<StringInterner>,
    address_space: Box<dyn IAddressSpace>,
    address_space_needs_reloading: bool,
    debug_info_index: DebugInfoIndex,
    binaries: HashMap<String, Arc<BinaryData>>,
    maps: RangeMap<Region>,
    backtraces: Vec<BacktraceStorageRef>,
    backtraces_storage: Vec<FrameId>,
    backtrace_to_id: HashMap<Vec<u64>, BacktraceId>,
    backtrace_remappings: HashMap<u64, BacktraceId>,
    group_stats: Vec<GroupStatistics>,
    operations: Vec<OperationId>,
    allocations: Vec<Allocation>,
    allocation_map: HashMap<DataPointer, AllocationId>,
    allocation_range_map: RangeMap<AllocationId>,
    allocation_range_map_dirty: bool,
    allocations_by_backtrace: HashMap<BacktraceId, Vec<AllocationId>>,
    frames: Vec<Frame>,
    frame_to_id: HashMap<Frame, FrameId>,
    frames_by_address: HashMap<u64, Range<usize>>,
    shared_ptr_backtraces: HashSet<BacktraceId>,
    shared_ptr_allocations: HashMap<DataPointer, AllocationId>,
    total_allocated: u64,
    total_allocated_count: u64,
    total_freed: u64,
    total_freed_count: u64,
    frame_skip_ranges: Vec<Range<u64>>,
    symbol_new_range: Range<u64>,
    marker: u32,
    mallopts: Vec<Mallopt>,
    timestamp_to_wall_clock: u64,
    is_little_endian: bool,
    mmap_operations: Vec<MmapOperation>,
    maximum_backtrace_depth: u32,
    previous_backtrace_on_thread: HashMap<u32, Vec<u64>>,
    string_id_map: HashMap<u32, StringId>,
}

fn address_to_frame<F: FnMut(Frame)>(
    address_space: &dyn IAddressSpace,
    interner: &mut StringInterner,
    address: u64,
    mut callback: F,
) {
    address_space.decode_symbol_while(address, &mut |frame| {
        let mut output = Frame::new_unknown(CodePointer::new(address));
        if let Some(str) = frame.library.take() {
            output.set_library(interner.get_or_intern(get_basename(&str)));
        }
        if let Some(str) = frame.demangled_name.take() {
            let str = clean_symbol(str);
            output.set_function(interner.get_or_intern(str));
        }
        if let Some(str) = frame.name.take() {
            output.set_raw_function(interner.get_or_intern(str));
        }
        if let Some(str) = frame.file.take() {
            output.set_source(interner.get_or_intern(str));
        }
        if let Some(value) = frame.line.take() {
            output.set_line(value as _);
        }
        if let Some(value) = frame.column.take() {
            output.set_column(value as _);
        }

        output.set_is_inline(frame.is_inline);

        callback(output);
        true
    });
}

trait PointerSize: Into<u64> {
    fn read<E: ByteOrder>(slice: &[u8]) -> Self;
}

impl PointerSize for u32 {
    #[inline]
    fn read<E: ByteOrder>(slice: &[u8]) -> Self {
        E::read_u32(slice)
    }
}

impl PointerSize for u64 {
    #[inline]
    fn read<E: ByteOrder>(slice: &[u8]) -> Self {
        E::read_u64(slice)
    }
}

fn get_basename(path: &str) -> &str {
    if path.is_empty() {
        return path;
    }

    let path = if path.as_bytes().last().cloned().unwrap() == b'/' {
        &path[0..path.len() - 1]
    } else {
        path
    };

    &path[path.rfind("/").map(|index| index + 1).unwrap_or(0)..]
}

impl Loader {
    pub fn new(header: HeaderBody, debug_info_index: DebugInfoIndex) -> Self {
        let address_space: Box<dyn IAddressSpace> = match &*header.arch {
            "arm" => Box::new(AddressSpace::<arch::arm::Arch>::new()),
            "x86_64" => Box::new(AddressSpace::<arch::amd64::Arch>::new()),
            "mips64" => Box::new(AddressSpace::<arch::mips64::Arch>::new()),
            "aarch64" => Box::new(AddressSpace::<arch::aarch64::Arch>::new()),
            _ => panic!("Unknown architecture: {}", header.arch),
        };

        let flags = header.flags;
        let timestamp = header.timestamp;
        let wall_clock_secs = header.wall_clock_secs;
        let wall_clock_nsecs = header.wall_clock_nsecs;

        let mut loader = Loader {
            id: header.id,
            header,
            interner: RefCell::new(StringInterner::new()),
            address_space,
            address_space_needs_reloading: true,
            debug_info_index,
            binaries: Default::default(),
            maps: RangeMap::new(),
            backtraces: Default::default(),
            backtraces_storage: Default::default(),
            backtrace_to_id: Default::default(),
            backtrace_remappings: Default::default(),
            group_stats: Default::default(),
            operations: Vec::with_capacity(100000),
            allocations: Vec::with_capacity(100000),
            allocation_map: Default::default(),
            allocation_range_map: RangeMap::new(),
            allocation_range_map_dirty: true,
            allocations_by_backtrace: Default::default(),
            frames: Default::default(),
            frame_to_id: Default::default(),
            frames_by_address: Default::default(),
            shared_ptr_backtraces: Default::default(),
            shared_ptr_allocations: Default::default(),
            total_allocated: 0,
            total_allocated_count: 0,
            total_freed: 0,
            total_freed_count: 0,
            frame_skip_ranges: Vec::with_capacity(4),
            symbol_new_range: -1_i64 as u64..0,
            marker: 0,
            mallopts: Default::default(),
            timestamp_to_wall_clock: 0,
            is_little_endian: (flags & HEADER_FLAG_IS_LITTLE_ENDIAN) != 0,
            mmap_operations: Default::default(),
            maximum_backtrace_depth: 0,
            previous_backtrace_on_thread: Default::default(),
            string_id_map: Default::default(),
        };

        loader.update_timestamp_to_wall_clock(timestamp, wall_clock_secs, wall_clock_nsecs);
        loader
    }

    fn update_timestamp_to_wall_clock(
        &mut self,
        timestamp: Timestamp,
        wall_clock_secs: u64,
        wall_clock_nsecs: u64,
    ) {
        self.timestamp_to_wall_clock = Timestamp::from_timespec(wall_clock_secs, wall_clock_nsecs)
            .as_usecs()
            .wrapping_sub(timestamp.as_usecs());
    }

    pub fn load_from_stream_without_debug_info<F: Read + Send + 'static>(
        fp: F,
    ) -> Result<Data, io::Error> {
        use std::iter;

        let empty: iter::Empty<&OsStr> = iter::empty();
        Loader::load_from_stream(fp, empty)
    }

    pub fn load_from_stream<
        F: Read + Send + 'static,
        D: AsRef<OsStr>,
        I: IntoIterator<Item = D>,
    >(
        fp: F,
        debug_symbols: I,
    ) -> Result<Data, io::Error> {
        debug!("Starting to load data...");

        let start_timestamp = Instant::now();
        let (header, event_stream) = parse_events(fp)?;

        let mut debug_info_index = DebugInfoIndex::new();
        for path in debug_symbols {
            debug_info_index.add(path.as_ref());
        }

        let mut loader = Loader::new(header, debug_info_index);

        for event in event_stream {
            let event = event?;
            loader.process(event);
        }

        let output = loader.finalize();
        let elapsed = start_timestamp.elapsed();
        info!(
            "Loaded data in {}s {:03}",
            elapsed.as_secs(),
            elapsed.subsec_millis()
        );
        Ok(output)
    }

    fn shift_timestamp(&self, timestamp: Timestamp) -> Timestamp {
        Timestamp::from_usecs(
            timestamp
                .as_usecs()
                .wrapping_add(self.timestamp_to_wall_clock),
        )
    }

    fn parse_flags(&self, backtrace: BacktraceId, flags: u32) -> AllocationFlags {
        let mut allocation_flags = AllocationFlags::empty();
        if self.shared_ptr_backtraces.contains(&backtrace) {
            allocation_flags |= AllocationFlags::IS_SHARED_PTR;
        }

        if flags & event::ALLOC_FLAG_PREV_IN_USE != 0 {
            allocation_flags |= AllocationFlags::IS_PREV_IN_USE;
        }

        if flags & event::ALLOC_FLAG_MMAPED != 0 {
            allocation_flags |= AllocationFlags::IS_MMAPED;
        }

        if flags & event::ALLOC_FLAG_NON_MAIN_ARENA != 0 {
            allocation_flags |= AllocationFlags::IN_NON_MAIN_ARENA;
        }

        if flags & event::ALLOC_FLAG_CALLOC != 0 {
            allocation_flags |= AllocationFlags::IS_CALLOC;
        }

        if self.shared_ptr_backtraces.contains(&backtrace) {
            allocation_flags |= AllocationFlags::IS_SHARED_PTR;
        }

        allocation_flags
    }

    fn handle_alloc(
        &mut self,
        timestamp: Timestamp,
        pointer: DataPointer,
        size: u64,
        backtrace: BacktraceId,
        thread: ThreadId,
        flags: u32,
        extra_usable_space: u32,
        preceding_free_space: u64,
    ) {
        let flags = self.parse_flags(backtrace, flags);
        let allocation_id = AllocationId::new(self.allocations.len() as _);
        let allocation = Allocation {
            pointer,
            timestamp,
            size,
            thread,
            backtrace,
            deallocation: None,
            reallocation: None,
            reallocated_from: None,
            flags,
            extra_usable_space,
            preceding_free_space: preceding_free_space as u32,
            marker: self.marker,
        };

        let entry = self.allocation_map.entry(pointer);
        if let hash_map::Entry::Occupied(entry) = entry {
            warn!(
                "Duplicate allocation of 0x{:016X}; old backtrace = {:?}, new backtrace = {:?}",
                pointer,
                self.allocations[entry.get().raw() as usize].backtrace,
                backtrace
            );
            return;
        }

        let group_stats = &mut self.group_stats[allocation.backtrace.raw() as usize];
        group_stats.first_allocation = cmp::min(group_stats.first_allocation, timestamp);
        group_stats.last_allocation = cmp::max(group_stats.last_allocation, timestamp);
        group_stats.min_size = cmp::min(group_stats.min_size, allocation.usable_size());
        group_stats.max_size = cmp::max(group_stats.max_size, allocation.usable_size());
        group_stats.alloc_count += 1;
        group_stats.alloc_size += allocation.usable_size();

        self.allocations.push(allocation);
        entry.or_insert(allocation_id);
        self.total_allocated += size;
        self.total_allocated_count += 1;

        let op = OperationId::new_allocation(allocation_id);
        self.operations.push(op);

        if flags.contains(AllocationFlags::IS_SHARED_PTR) {
            self.shared_ptr_allocations.insert(pointer, allocation_id);
        }

        self.allocation_range_map_dirty = true;
        self.allocations_by_backtrace
            .get_mut(&backtrace)
            .unwrap()
            .push(allocation_id);
    }

    fn handle_free(
        &mut self,
        timestamp: Timestamp,
        pointer: DataPointer,
        backtrace: Option<BacktraceId>,
        thread: ThreadId,
    ) {
        let allocation_id = match self.allocation_map.remove(&pointer) {
            Some(id) => id,
            None => {
                debug!(
                    "Unknown deallocation of 0x{:016X} at backtrace = {:?}",
                    pointer, backtrace
                );
                return;
            }
        };

        let allocation = &mut self.allocations[allocation_id.raw() as usize];
        allocation.deallocation = Some(Deallocation {
            timestamp,
            thread,
            backtrace,
        });
        self.total_freed += allocation.size;
        self.total_freed_count += 1;
        let group_stats = &mut self.group_stats[allocation.backtrace.raw() as usize];
        group_stats.free_count += 1;
        group_stats.free_size += allocation.usable_size();

        let op = OperationId::new_deallocation(allocation_id);
        self.operations.push(op);

        if allocation.is_shared_ptr() {
            self.shared_ptr_allocations.remove(&allocation.pointer);
        }

        self.allocation_range_map_dirty = true;
    }

    fn handle_realloc(
        &mut self,
        timestamp: Timestamp,
        old_pointer: DataPointer,
        new_pointer: DataPointer,
        size: u64,
        backtrace: BacktraceId,
        thread: ThreadId,
        flags: u32,
        extra_usable_space: u32,
        preceding_free_space: u64,
    ) {
        let allocation_id = match self.allocation_map.remove(&old_pointer) {
            Some(id) => id,
            None => return,
        };

        let flags = self.parse_flags(backtrace, flags);
        let reallocation_id = AllocationId::new(self.allocations.len() as _);
        {
            let allocation = &mut self.allocations[allocation_id.raw() as usize];
            assert!(!allocation.is_shared_ptr());

            allocation.deallocation = Some(Deallocation {
                timestamp,
                thread,
                backtrace: Some(backtrace),
            });
            allocation.reallocation = Some(reallocation_id);
            self.total_freed += allocation.size;
            self.total_freed_count += 1;
            self.group_stats[allocation.backtrace.raw() as usize].free_count += 1;
            self.group_stats[allocation.backtrace.raw() as usize].free_size +=
                allocation.usable_size();
        }

        let reallocation = Allocation {
            pointer: new_pointer,
            timestamp,
            size,
            thread,
            backtrace,
            deallocation: None,
            reallocation: None,
            reallocated_from: Some(allocation_id),
            flags,
            extra_usable_space,
            preceding_free_space: preceding_free_space as u32,
            marker: self.marker,
        };

        let entry = self.allocation_map.entry(new_pointer);
        if let hash_map::Entry::Occupied(entry) = entry {
            warn!( "Duplicate allocation (during realloc) of 0x{:016X}; old backtrace = {:?}, new backtrace = {:?}", new_pointer, self.allocations[ entry.get().raw() as usize ].backtrace, backtrace );
            return;
        }

        let group_stats = &mut self.group_stats[reallocation.backtrace.raw() as usize];
        group_stats.first_allocation = cmp::min(group_stats.first_allocation, timestamp);
        group_stats.last_allocation = cmp::max(group_stats.last_allocation, timestamp);
        group_stats.min_size = cmp::min(group_stats.min_size, reallocation.usable_size());
        group_stats.max_size = cmp::max(group_stats.max_size, reallocation.usable_size());
        group_stats.alloc_count += 1;
        group_stats.alloc_size += reallocation.usable_size();

        self.allocations.push(reallocation);
        entry.or_insert(reallocation_id);
        self.total_allocated += size;
        self.total_allocated_count += 1;

        let op = OperationId::new_reallocation(reallocation_id);
        self.operations.push(op);

        self.allocation_range_map_dirty = true;
        self.allocations_by_backtrace
            .get_mut(&backtrace)
            .unwrap()
            .push(allocation_id);
    }

    pub(crate) fn interner(&mut self) -> &mut StringInterner {
        self.interner.get_mut()
    }

    pub(crate) fn lookup_backtrace(&mut self, backtrace: u64) -> Option<BacktraceId> {
        let backtrace_id = self.backtrace_remappings.get(&backtrace).cloned()?;
        self.allocations_by_backtrace
            .entry(backtrace_id)
            .or_insert(Vec::new());
        Some(backtrace_id)
    }

    fn scan<P: PointerSize, B: ByteOrder>(&self, base_address: u64, data: &[u8]) {
        assert_eq!(data.len() % mem::size_of::<P>(), 0);
        for (index, subslice) in data.chunks_exact(mem::size_of::<P>()).enumerate() {
            let value: u64 = P::read::<B>(subslice).into();
            let allocation_id = match self.shared_ptr_allocations.get(&value) {
                Some(&value) => value,
                None => continue,
            };

            let container_address = base_address + (mem::size_of::<P>() * index) as u64;

            if let Some(&container_allocation_id) =
                self.allocation_range_map.get_value(container_address)
            {
                let allocation: &Allocation = &self.allocations[allocation_id.raw() as usize];
                let container_allocation =
                    &self.allocations[container_allocation_id.raw() as usize];
                assert!(!allocation.was_deallocated());
                assert!(!container_allocation.was_deallocated());

                trace!(
                    "Found an instance of shared pointer #{} (0x{:016X}) at 0x{:016X} (0x{:016X} + {}, allocation #{})",
                    allocation_id.raw(),
                    value,
                    container_address,
                    base_address,
                    mem::size_of::< P >() * index,
                    container_allocation_id.raw()
                );

                // TODO
            }
        }
    }

    fn reload_address_space(&mut self) {
        if !self.address_space_needs_reloading {
            return;
        }

        self.address_space_needs_reloading = false;
        let binaries = &self.binaries;
        let debug_info_index = &mut self.debug_info_index;
        let regions: Vec<Region> = self.maps.values().cloned().collect();
        self.address_space.reload(regions, &mut |region, handle| {
            handle.should_load_frame_descriptions(false);

            let basename = get_basename(&region.name);
            let debug_binary_data = if let Some(binary_data) = binaries.get(&region.name).cloned() {
                let debug_binary_data = debug_info_index.get(
                    &basename,
                    binary_data.debuglink(),
                    binary_data.build_id(),
                );
                handle.set_binary(binary_data);
                debug_binary_data
            } else {
                debug_info_index.get(&basename, None, None)
            };

            if let Some(debug_binary_data) = debug_binary_data {
                handle.set_debug_binary(debug_binary_data.clone());
            }
        });
    }

    fn scan_for_symbols(&mut self, binary_data: &BinaryData) {
        if self.maps.is_empty() {
            return;
        }

        ::nwind::Symbols::each_from_binary_data(&binary_data, |range, name| {
            if name == "_Znwm" {
                let region = self
                    .maps
                    .values()
                    .find(|region| region.name == binary_data.name());

                if let Some(region) = region {
                    self.symbol_new_range = region.start + range.start..region.start + range.end;
                }
            }
        });
    }

    fn handle_backtrace(&mut self, id: BacktraceId, potentially_call_to_new: bool) {
        let (offset, length) = self.backtraces[id.raw() as usize];
        self.maximum_backtrace_depth = cmp::max(self.maximum_backtrace_depth, length as _);

        if potentially_call_to_new {
            let interner = self.interner.get_mut();
            let frames = &self.frames;
            let mut iter = self.backtraces_storage[offset as usize..]
                .iter()
                .rev()
                .flat_map(|&id| {
                    frames[id]
                        .raw_function()
                        .and_then(|id| interner.resolve(id))
                });
            if let Some(name) = iter.next() {
                if name == "_ZNSt16_Sp_counted_baseILN9__gnu_cxx12_Lock_policyE2EEC4Ev" {
                    self.shared_ptr_backtraces.insert(id);
                }
            }
        }

        self.allocations_by_backtrace
            .entry(id)
            .or_insert(Vec::new());

        assert_eq!(self.group_stats.len(), id.raw() as usize);
        self.group_stats.push(Default::default());
    }

    fn add_backtrace<F>(
        &mut self,
        raw_id: u64,
        addresses: Cow<[u64]>,
        mut callback: F,
    ) -> Option<BacktraceId>
    where
        F: FnMut(FrameId, bool),
    {
        self.reload_address_space();

        let mut is_call_to_new = false;
        let to_skip = addresses
            .iter()
            .take_while(|&&address| {
                let address = address - 1;
                for range in &self.frame_skip_ranges {
                    if address >= range.start && address <= range.end {
                        return true;
                    }
                }

                if address >= self.symbol_new_range.start && address < self.symbol_new_range.end {
                    is_call_to_new = true;
                    return true;
                }

                false
            })
            .count();

        if let Some(target_id) = self.backtrace_to_id.get(&addresses[to_skip..]).cloned() {
            self.backtrace_remappings.insert(raw_id, target_id);
            return None;
        }

        let mut addresses = addresses.into_owned();
        addresses.drain(0..to_skip);

        let backtrace_storage_offset = self.backtraces_storage.len();
        let backtrace_storage = &mut self.backtraces_storage;
        let frames = &mut self.frames;
        let frame_to_id = &mut self.frame_to_id;
        let mut interner = self.interner.get_mut();

        for &address in &addresses {
            let address = if address > 0 { address - 1 } else { 0 };
            if let Some(range) = self.frames_by_address.get(&address).cloned() {
                for index in range {
                    let frame_id = backtrace_storage[index];
                    backtrace_storage.push(frame_id);
                    callback(frame_id, false);
                }
            } else {
                let offset = backtrace_storage.len();
                address_to_frame(&*self.address_space, &mut interner, address, |frame| {
                    let (frame_id, is_new) = if let Some(&frame_id) = frame_to_id.get(&frame) {
                        (frame_id, false)
                    } else {
                        let frame_id = frames.len();
                        frame_to_id.insert(frame.clone(), frame_id);
                        frames.push(frame);
                        (frame_id, true)
                    };

                    callback(frame_id, is_new);
                    backtrace_storage.push(frame_id);
                });

                self.frames_by_address
                    .insert(address, offset..backtrace_storage.len());
            }
        }

        let id = BacktraceId::new(self.backtraces.len() as _);
        self.backtrace_remappings.insert(raw_id, id);

        let backtrace_length = backtrace_storage.len() - backtrace_storage_offset;
        let backtrace_storage_ref = (backtrace_storage_offset as _, backtrace_length as _);

        self.backtrace_to_id.insert(addresses, id);
        self.backtraces.push(backtrace_storage_ref);

        self.handle_backtrace(id, is_call_to_new);
        Some(id)
    }

    pub(crate) fn expand_partial_backtrace(
        previous_backtrace_on_thread: &mut HashMap<u32, Vec<u64>>,
        thread: u32,
        frames_invalidated: FramesInvalidated,
        partial_addresses: impl ExactSizeIterator<Item = u64>,
    ) -> Vec<u64> {
        match frames_invalidated {
            FramesInvalidated::All => {
                let mut addresses = Vec::new();
                mem::swap(
                    previous_backtrace_on_thread
                        .entry(thread)
                        .or_insert(Vec::new()),
                    &mut addresses,
                );

                addresses.clear();
                addresses.extend(partial_addresses);
                addresses
            }
            FramesInvalidated::Some(frames_invalidated) => {
                let old_addresses = previous_backtrace_on_thread
                    .entry(thread)
                    .or_insert(Vec::new());
                let new_iter = partial_addresses;
                let old_iter = old_addresses
                    .iter()
                    .cloned()
                    .skip(frames_invalidated as usize);
                new_iter.chain(old_iter).collect()
            }
        }
    }

    pub(crate) fn process_backtrace_event<F>(
        &mut self,
        event: Event,
        callback: F,
    ) -> Option<BacktraceId>
    where
        F: FnMut(FrameId, bool),
    {
        match event {
            Event::PartialBacktrace {
                id: raw_id,
                thread,
                frames_invalidated,
                addresses: partial_addresses,
            } => {
                let addresses = Self::expand_partial_backtrace(
                    &mut self.previous_backtrace_on_thread,
                    thread,
                    frames_invalidated,
                    partial_addresses.iter().cloned(),
                );
                let backtrace_id =
                    self.add_backtrace(raw_id, addresses.as_slice().into(), callback);
                mem::replace(
                    self.previous_backtrace_on_thread.get_mut(&thread).unwrap(),
                    addresses,
                );

                backtrace_id
            }
            Event::PartialBacktrace32 {
                id: raw_id,
                thread,
                frames_invalidated,
                addresses: partial_addresses,
            } => {
                let addresses = Self::expand_partial_backtrace(
                    &mut self.previous_backtrace_on_thread,
                    thread,
                    frames_invalidated,
                    partial_addresses.iter().cloned().map(|value| value as u64),
                );
                let backtrace_id =
                    self.add_backtrace(raw_id, addresses.as_slice().into(), callback);
                mem::replace(
                    self.previous_backtrace_on_thread.get_mut(&thread).unwrap(),
                    addresses,
                );

                backtrace_id
            }
            Event::Backtrace {
                id: raw_id,
                addresses,
            } => self.add_backtrace(raw_id, addresses, callback),
            _ => {
                unreachable!();
            }
        }
    }

    pub(crate) fn get_frame(&self, id: FrameId) -> &Frame {
        &self.frames[id]
    }

    pub fn process(&mut self, event: Event) {
        match event {
            Event::Header(header) => {
                assert_eq!(header.id, self.header.id);
                assert_eq!(header.initial_timestamp, self.header.initial_timestamp);
            }
            Event::File {
                ref path,
                ref contents,
                ..
            } if path == "/proc/self/maps" => {
                let contents = String::from_utf8_lossy(&contents);
                trace!("/proc/self/maps:\n{}", contents);

                let mut maps = Vec::new();

                self.frame_skip_ranges.clear();
                for region in parse_maps(&contents) {
                    if region.name.contains("libntracker_preload")
                        || region.name.contains("libmemory_profiler")
                    {
                        if self
                            .frame_skip_ranges
                            .last()
                            .map(|last_range| last_range.end == region.start)
                            .unwrap_or(false)
                        {
                            let mut last_range = self.frame_skip_ranges.last_mut().unwrap();
                            last_range.end = region.end;
                        } else {
                            self.frame_skip_ranges.push(region.start..region.end);
                        }
                    }

                    maps.push((region.start..region.end, region));
                }

                for range in &self.frame_skip_ranges {
                    debug!("Skip range: 0x{:016X}-0x{:016X}", range.start, range.end);
                }

                self.maps = RangeMap::from_vec(maps);
                let binaries: Vec<_> = self.binaries.values().cloned().collect();
                for binary_data in binaries {
                    self.scan_for_symbols(&binary_data);
                }

                self.address_space_needs_reloading = true;
            }
            Event::File {
                ref path,
                ref contents,
                ..
            } => {
                if !contents.starts_with(b"\x7FELF") {
                    return;
                }

                trace!("File: {}", path);
                if let Ok(binary_data) =
                    BinaryData::load_from_owned_bytes(&path, contents.clone().into_owned())
                {
                    self.scan_for_symbols(&binary_data);
                    self.binaries
                        .insert(path.deref().to_owned(), Arc::new(binary_data));
                }
            }
            event @ Event::PartialBacktrace { .. }
            | event @ Event::PartialBacktrace32 { .. }
            | event @ Event::Backtrace { .. } => {
                self.process_backtrace_event(event, |_, _| {});
            }
            Event::String { id, string } => {
                let target_id = self.interner.get_mut().get_or_intern(string);
                self.string_id_map.insert(id, target_id);
            }
            Event::DecodedFrame {
                address,
                library,
                raw_function,
                function,
                source,
                line,
                column,
                is_inline,
            } => {
                let mut frame = Frame::new_unknown(CodePointer::new(address));
                if library != 0xFFFFFFFF {
                    frame.set_library(*self.string_id_map.get(&library).unwrap());
                }
                if raw_function != 0xFFFFFFFF {
                    frame.set_raw_function(*self.string_id_map.get(&raw_function).unwrap());
                }
                if function != 0xFFFFFFFF {
                    frame.set_function(*self.string_id_map.get(&function).unwrap());
                }
                if source != 0xFFFFFFFF {
                    frame.set_source(*self.string_id_map.get(&source).unwrap());
                }
                if line != 0xFFFFFFFF {
                    frame.set_line(line);
                }
                if column != 0xFFFFFFFF {
                    frame.set_column(column);
                }

                frame.set_is_inline(is_inline);
                self.frames.push(frame);
            }
            Event::DecodedBacktrace { frames } => {
                let id = BacktraceId::new(self.backtraces.len() as _);
                self.backtrace_remappings.insert(id.raw() as _, id);

                let backtrace_storage_offset = self.backtraces_storage.len();
                self.backtraces_storage
                    .extend(frames.iter().cloned().map(|id| id as usize));
                let backtrace_length = self.backtraces_storage.len() - backtrace_storage_offset;
                let backtrace_storage_ref = (backtrace_storage_offset as _, backtrace_length as _);
                self.backtraces.push(backtrace_storage_ref);
                self.handle_backtrace(id, true);
            }
            Event::Alloc {
                timestamp,
                allocation:
                    AllocBody {
                        pointer,
                        size,
                        backtrace,
                        thread,
                        flags,
                        extra_usable_space,
                        preceding_free_space,
                    },
            } => {
                let timestamp = self.shift_timestamp(timestamp);
                let backtrace = self.lookup_backtrace(backtrace).unwrap();
                self.handle_alloc(
                    timestamp,
                    pointer,
                    size,
                    backtrace,
                    thread,
                    flags,
                    extra_usable_space,
                    preceding_free_space,
                );
            }
            Event::Realloc {
                timestamp,
                old_pointer,
                allocation:
                    AllocBody {
                        pointer,
                        size,
                        backtrace,
                        thread,
                        flags,
                        extra_usable_space,
                        preceding_free_space,
                    },
            } => {
                let timestamp = self.shift_timestamp(timestamp);
                let backtrace = self.lookup_backtrace(backtrace).unwrap();
                self.handle_realloc(
                    timestamp,
                    old_pointer,
                    pointer,
                    size,
                    backtrace,
                    thread,
                    flags,
                    extra_usable_space,
                    preceding_free_space,
                );
            }
            Event::Free {
                timestamp,
                pointer,
                backtrace,
                thread,
            } => {
                let timestamp = self.shift_timestamp(timestamp);
                let backtrace = self.lookup_backtrace(backtrace);
                self.handle_free(timestamp, pointer, backtrace, thread);
            }
            Event::MemoryMap {
                timestamp,
                pointer,
                length,
                backtrace,
                requested_address,
                mmap_protection,
                mmap_flags,
                file_descriptor,
                thread,
                offset,
            } => {
                let timestamp = self.shift_timestamp(timestamp);
                let backtrace = self.lookup_backtrace(backtrace).unwrap();
                let mmap = MemoryMap {
                    timestamp,
                    pointer,
                    length,
                    backtrace,
                    requested_address,
                    mmap_protection: ProtectionFlags(mmap_protection),
                    mmap_flags: MapFlags(mmap_flags),
                    file_descriptor,
                    thread,
                    offset,
                };

                self.mmap_operations.push(MmapOperation::Mmap(mmap));
            }
            Event::MemoryUnmap {
                timestamp,
                pointer,
                length,
                backtrace,
                thread,
            } => {
                let timestamp = self.shift_timestamp(timestamp);
                let backtrace = self.lookup_backtrace(backtrace).unwrap();
                let munmap = MemoryUnmap {
                    timestamp,
                    pointer,
                    length,
                    backtrace,
                    thread,
                };

                self.mmap_operations.push(MmapOperation::Munmap(munmap));
            }
            Event::Mallopt {
                timestamp,
                backtrace,
                thread,
                param,
                value,
                result,
            } => {
                let timestamp = self.shift_timestamp(timestamp);
                let backtrace = self.lookup_backtrace(backtrace).unwrap();
                let kind = param.into();
                let mallopt = Mallopt {
                    timestamp,
                    backtrace,
                    thread,
                    kind,
                    value,
                    result,
                };
                self.mallopts.push(mallopt);
            }
            Event::Environ { .. } => {
                // TODO
            }
            Event::WallClock {
                timestamp,
                sec,
                nsec,
            } => {
                self.update_timestamp_to_wall_clock(timestamp, sec, nsec);
            }
            Event::Marker { value } => {
                self.marker = value;
            }
            Event::MemoryDump {
                address,
                length,
                data,
            } => {
                if true {
                    // TODO
                    return;
                }

                if self.allocation_range_map_dirty {
                    let mut allocations: Vec<(Range<u64>, AllocationId)> =
                        Vec::with_capacity(self.allocations.len());
                    for (allocation_id, allocation) in self.allocations.iter().enumerate() {
                        let allocation_id = AllocationId::new(allocation_id as _);
                        if allocation.was_deallocated() {
                            continue;
                        }

                        allocations.push((
                            allocation.pointer..allocation.pointer + allocation.size,
                            allocation_id,
                        ));
                    }

                    let count = allocations.len();
                    self.allocation_range_map = RangeMap::from_vec(allocations);
                    self.allocation_range_map_dirty = false;
                    assert_eq!(count, self.allocation_range_map.len());
                }

                let length = length as usize;
                assert_eq!(data.len(), length);
                match (self.header.pointer_size, self.is_little_endian) {
                    (4, false) => self.scan::<u32, BigEndian>(address, &data),
                    (4, true) => self.scan::<u32, LittleEndian>(address, &data),
                    (8, false) => self.scan::<u64, BigEndian>(address, &data),
                    (8, true) => self.scan::<u64, LittleEndian>(address, &data),
                    _ => unreachable!(),
                }
            }
            Event::GroupStatistics {
                backtrace,
                first_allocation,
                last_allocation,
                free_count,
                free_size,
                min_size,
                max_size,
            } => {
                let first_allocation = self.shift_timestamp(first_allocation);
                let last_allocation = self.shift_timestamp(last_allocation);
                let backtrace = self.lookup_backtrace(backtrace).unwrap();
                let group_stats = &mut self.group_stats[backtrace.raw() as usize];
                group_stats.first_allocation =
                    cmp::min(group_stats.first_allocation, first_allocation);
                group_stats.last_allocation =
                    cmp::max(group_stats.last_allocation, last_allocation);
                group_stats.min_size = cmp::min(group_stats.min_size, min_size);
                group_stats.max_size = cmp::max(group_stats.max_size, max_size);
                group_stats.alloc_count += free_count;
                group_stats.alloc_size += free_size;
                group_stats.free_count += free_count;
                group_stats.free_size += free_size;
            }
        }
    }

    pub fn finalize(mut self) -> Data {
        let initial_timestamp = self.shift_timestamp(self.header.initial_timestamp);
        let indices: Vec<AllocationId> = (0..self.allocations.len())
            .into_iter()
            .map(|id| AllocationId::new(id as _))
            .collect();

        for (raw_backtrace_id, stats) in self.group_stats.iter().enumerate() {
            let (backtrace_offset, backtrace_len) = self.backtraces[raw_backtrace_id];
            for &frame_id in &self.backtraces_storage
                [backtrace_offset as usize..(backtrace_offset + backtrace_len) as usize]
            {
                self.frames[frame_id].increment_count(stats.alloc_count);
            }
        }

        fn cmp_by_time(
            allocations: &[Allocation],
            a_id: AllocationId,
            b_id: AllocationId,
        ) -> std::cmp::Ordering {
            let a_alloc = &allocations[a_id.raw() as usize];
            let b_alloc = &allocations[b_id.raw() as usize];
            a_alloc
                .timestamp
                .cmp(&b_alloc.timestamp)
                .then_with(|| a_id.raw().cmp(&b_id.raw()))
        }

        let mut sorted_by_timestamp = indices.clone();
        sorted_by_timestamp.sort_by(|&a_id, &b_id| cmp_by_time(&self.allocations, a_id, b_id));

        let mut sorted_by_address = indices.clone();
        sorted_by_address.sort_by_key(|index| self.allocations[index.raw() as usize].pointer);

        let mut sorted_by_size = indices;
        sorted_by_size.sort_by_key(|index| self.allocations[index.raw() as usize].size);

        self.allocations.shrink_to_fit();
        self.operations.shrink_to_fit();
        self.frames.shrink_to_fit();
        self.backtraces.shrink_to_fit();
        self.backtraces_storage.shrink_to_fit();
        self.mallopts.shrink_to_fit();
        self.mmap_operations.shrink_to_fit();
        self.group_stats.shrink_to_fit();

        let mut allocations_by_backtrace = DenseVecVec::new();
        let mut index: Vec<_> = self.allocations_by_backtrace.into_iter().collect();
        index.sort_by_key(|&(k, _)| k);

        let allocations = &self.allocations;
        for (backtrace_id, mut allocation_ids) in index {
            allocation_ids.sort_by(|&a_id, &b_id| cmp_by_time(allocations, a_id, b_id));
            let index = allocations_by_backtrace.push(allocation_ids);
            assert_eq!(index, backtrace_id.raw() as _);
        }

        allocations_by_backtrace.shrink_to_fit();

        let last_timestamp = self
            .group_stats
            .iter()
            .map(|stats| stats.last_allocation)
            .max()
            .unwrap_or(initial_timestamp);
        Data {
            id: self.id,
            initial_timestamp,
            last_timestamp,
            executable: String::from_utf8_lossy(&self.header.executable).into_owned(),
            architecture: self.header.arch,
            pointer_size: self.header.pointer_size as _,
            interner: self.interner.into_inner(),
            allocations: self.allocations,
            sorted_by_timestamp,
            sorted_by_address,
            sorted_by_size,
            operations: self.operations,
            frames: self.frames,
            backtraces: self.backtraces,
            backtraces_storage: self.backtraces_storage,
            allocations_by_backtrace,
            total_allocated: self.total_allocated,
            total_allocated_count: self.total_allocated_count,
            total_freed: self.total_freed,
            total_freed_count: self.total_freed_count,
            mallopts: self.mallopts,
            mmap_operations: self.mmap_operations,
            maximum_backtrace_depth: self.maximum_backtrace_depth,
            group_stats: self.group_stats,
        }
    }
}
