use std::borrow::{Borrow, Cow};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::iter::FusedIterator;
use std::num::NonZeroU32;
use std::ops::Range;

use hashbrown::HashMap;
use string_interner;

use crate::frame::Frame;
use crate::tree::Tree;
use crate::tree_printer::dump_tree;
use crate::util::{table_to_string, ReadableSize};
use crate::vecvec::DenseVecVec;

pub use common::event::DataId;
pub use common::Timestamp;

pub type StringInterner = string_interner::StringInterner<StringId>;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StringId(NonZeroU32);

impl string_interner::Symbol for StringId {
    #[inline]
    fn from_usize(value: usize) -> Self {
        unsafe { StringId(NonZeroU32::new_unchecked((value + 1) as u32)) }
    }

    #[inline]
    fn to_usize(self) -> usize {
        self.0.get() as usize - 1
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct AllocationId(u64);

impl AllocationId {
    pub(crate) fn new(raw: u64) -> Self {
        AllocationId(raw)
    }

    pub fn raw(&self) -> u64 {
        self.0
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct OperationId(u64);

impl OperationId {
    #[inline]
    pub(crate) fn new_allocation(id: AllocationId) -> Self {
        OperationId(id.0)
    }

    #[inline]
    pub(crate) fn new_deallocation(id: AllocationId) -> Self {
        OperationId((1 << 62) | id.0)
    }

    #[inline]
    pub(crate) fn new_reallocation(id: AllocationId) -> Self {
        OperationId((2 << 62) | id.0)
    }

    #[inline]
    pub fn is_allocation(&self) -> bool {
        (self.0 >> 62) == 0
    }

    #[allow(dead_code)]
    #[inline]
    pub fn is_deallocation(&self) -> bool {
        (self.0 >> 62) == 1
    }

    #[inline]
    pub fn is_reallocation(&self) -> bool {
        (self.0 >> 62) == 2
    }

    #[inline]
    pub fn id(&self) -> AllocationId {
        AllocationId(self.0 & !(3 << 62))
    }
}

#[test]
fn test_operation_id() {
    let max = AllocationId::new(0x3fff_ffff_ffff_ffff);
    let id = OperationId::new_allocation(max);
    assert!(id.is_allocation());
    assert!(!id.is_reallocation());
    assert!(!id.is_deallocation());
    assert_eq!(id.id(), max);

    let id = OperationId::new_deallocation(max);
    assert!(!id.is_allocation());
    assert!(!id.is_reallocation());
    assert!(id.is_deallocation());
    assert_eq!(id.id(), max);

    let id = OperationId::new_reallocation(max);
    assert!(!id.is_allocation());
    assert!(id.is_reallocation());
    assert!(!id.is_deallocation());
    assert_eq!(id.id(), max);
}

pub struct Data {
    pub(crate) id: DataId,
    pub(crate) initial_timestamp: Timestamp,
    pub(crate) last_timestamp: Timestamp,
    pub(crate) executable: String,
    pub(crate) architecture: String,
    pub(crate) pointer_size: u64,
    pub(crate) interner: StringInterner,
    pub(crate) operations: Vec<OperationId>,
    pub(crate) allocations: Vec<Allocation>,
    pub(crate) sorted_by_timestamp: Vec<AllocationId>,
    pub(crate) sorted_by_address: Vec<AllocationId>,
    pub(crate) sorted_by_size: Vec<AllocationId>,
    pub(crate) frames: Vec<Frame>,
    pub(crate) backtraces: Vec<BacktraceStorageRef>,
    pub(crate) backtraces_storage: Vec<FrameId>,
    pub(crate) allocations_by_backtrace: DenseVecVec<AllocationId>,
    pub(crate) total_allocated: u64,
    pub(crate) total_allocated_count: u64,
    pub(crate) total_freed: u64,
    pub(crate) total_freed_count: u64,
    pub(crate) mallopts: Vec<Mallopt>,
    pub(crate) mmap_operations: Vec<MmapOperation>,
    pub(crate) maximum_backtrace_depth: u32,
    pub(crate) group_stats: Vec<GroupStatistics>,
}

pub type DataPointer = u64;
pub type FrameId = usize;
pub type ThreadId = u32;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CodePointer(u64);

impl CodePointer {
    pub fn new(address: u64) -> Self {
        CodePointer(address)
    }

    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for CodePointer {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{:016X}", self.0)
    }
}

impl From<CodePointer> for u64 {
    #[inline]
    fn from(value: CodePointer) -> Self {
        value.0
    }
}

impl From<u64> for CodePointer {
    #[inline]
    fn from(value: u64) -> Self {
        CodePointer(value)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BacktraceId(u32);

impl BacktraceId {
    #[inline]
    pub fn new(raw_id: u32) -> Self {
        BacktraceId(raw_id)
    }

    #[inline]
    pub fn raw(&self) -> u32 {
        self.0
    }
}

pub type BacktraceStorageRef = (u32, u32);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SourceKey {
    Address(CodePointer),
    Location(StringId, u32),
    Function(StringId),
}

bitflags! {
    pub struct AllocationFlags: u8 {
        const IS_PREV_IN_USE    = 1 << 0;
        const IS_MMAPED         = 1 << 1;
        const IN_NON_MAIN_ARENA = 1 << 2;
        const IS_SHARED_PTR     = 1 << 6;
        const IS_CALLOC         = 1 << 7;
    }
}

#[derive(Debug)]
pub struct Allocation {
    pub pointer: DataPointer,
    pub timestamp: Timestamp,
    pub thread: ThreadId,
    pub size: u64,
    pub backtrace: BacktraceId,
    pub deallocation: Option<Deallocation>,
    pub reallocation: Option<AllocationId>,
    pub reallocated_from: Option<AllocationId>,
    pub flags: AllocationFlags,
    pub extra_usable_space: u32,
    pub marker: u32,
    pub preceding_free_space: u32,
}

#[derive(Debug)]
pub struct GroupStatistics {
    pub first_allocation: Timestamp,
    pub last_allocation: Timestamp,
    pub alloc_count: u64,
    pub alloc_size: u64,
    pub free_count: u64,
    pub free_size: u64,
    pub min_size: u64,
    pub max_size: u64,
}

impl Default for GroupStatistics {
    fn default() -> Self {
        GroupStatistics {
            first_allocation: Timestamp::max(),
            last_allocation: Timestamp::min(),
            alloc_count: 0,
            alloc_size: 0,
            free_count: 0,
            free_size: 0,
            min_size: -1_i64 as u64,
            max_size: 0,
        }
    }
}

macro_rules! enum_primitive {
    (#[$($attr:tt)*] pub enum $name:ident {
        $other_name:ident( $primitive:ty ),
        $($variant:ident = $value:expr),*
    }) => {
        #[$($attr)+]
        pub enum $name {
            $($variant,)*
            $other_name( $primitive )
        }

        impl $name {
            pub fn raw( &self ) -> $primitive {
                match *self {
                    $name::$other_name( value ) => value,
                    $(
                        $name::$variant => $value
                    ),*
                }
            }
        }

        impl From< $primitive > for $name {
            fn from( value: $primitive ) -> Self {
                $(
                    if value == $value {
                        return $name::$variant;
                    };
                )*

                $name::$other_name( value )
            }
        }

        impl From< $name > for $primitive {
            fn from( value: $name ) -> Self {
                value.raw()
            }
        }
    };
}

enum_primitive! {
    #[derive(Debug)]
    pub enum MalloptKind {
        Other( i32 ),
        TrimThreshold   = -1,
        TopPad          = -2,
        MmapThreshold   = -3,
        MmapMax         = -4,
        CheckAction     = -5,
        Perturb         = -6,
        ArenaTest       = -7,
        ArenaMax        = -8
    }
}

#[derive(Debug)]
pub struct Mallopt {
    pub timestamp: Timestamp,
    pub backtrace: BacktraceId,
    pub thread: ThreadId,
    pub kind: MalloptKind,
    pub value: i32,
    pub result: i32,
}

impl Allocation {
    #[inline]
    pub fn was_deallocated(&self) -> bool {
        self.deallocation.is_some()
    }

    #[inline]
    pub fn is_shared_ptr(&self) -> bool {
        self.flags.contains(AllocationFlags::IS_SHARED_PTR)
    }

    #[inline]
    pub fn in_non_main_arena(&self) -> bool {
        self.flags.contains(AllocationFlags::IN_NON_MAIN_ARENA)
    }

    #[inline]
    pub fn in_main_arena(&self) -> bool {
        !self.in_non_main_arena()
    }

    #[inline]
    pub fn is_mmaped(&self) -> bool {
        self.flags.contains(AllocationFlags::IS_MMAPED)
    }

    #[inline]
    pub fn usable_size(&self) -> u64 {
        self.size + self.extra_usable_space as u64
    }

    #[inline]
    pub fn actual_range(&self, data: &Data) -> Range<u64> {
        let multiplier = if self.is_mmaped() { 2 } else { 1 };
        self.pointer - data.pointer_size * multiplier
            ..self.pointer + self.size + self.extra_usable_space as u64
    }
}

#[derive(Debug)]
pub struct Deallocation {
    pub timestamp: Timestamp,
    pub thread: ThreadId,
    pub backtrace: Option<BacktraceId>,
}

#[derive(Debug)]
pub enum Operation<'a> {
    Allocation {
        allocation: &'a Allocation,
        allocation_id: AllocationId,
    },
    Deallocation {
        allocation: &'a Allocation,
        allocation_id: AllocationId,
        deallocation: &'a Deallocation,
    },
    Reallocation {
        new_allocation: &'a Allocation,
        deallocation: &'a Deallocation,
        old_allocation: &'a Allocation,
    },
}

#[derive(Debug)]
pub struct MemoryMap {
    pub timestamp: Timestamp,
    pub pointer: DataPointer,
    pub length: u64,
    pub backtrace: BacktraceId,
    pub requested_address: u64,
    pub mmap_protection: ProtectionFlags,
    pub mmap_flags: MapFlags,
    pub file_descriptor: u32,
    pub thread: ThreadId,
    pub offset: u64,
}

#[derive(Debug)]
pub struct MemoryUnmap {
    pub timestamp: Timestamp,
    pub pointer: DataPointer,
    pub length: u64,
    pub backtrace: BacktraceId,
    pub thread: ThreadId,
}

#[derive(Debug)]
pub enum MmapOperation {
    Mmap(MemoryMap),
    Munmap(MemoryUnmap),
}

#[derive(Copy, Clone, Debug)]
pub struct ProtectionFlags(pub(crate) u32);

impl ProtectionFlags {
    pub fn is_readable(&self) -> bool {
        self.0 & 0x1 != 0
    }

    pub fn is_writable(&self) -> bool {
        self.0 & 0x2 != 0
    }

    pub fn is_executable(&self) -> bool {
        self.0 & 0x4 != 0
    }

    pub fn is_semaphore(&self) -> bool {
        self.0 & 0x8 != 0
    }

    pub fn grows_down(&self) -> bool {
        self.0 & 0x01000000 != 0
    }

    pub fn grows_up(&self) -> bool {
        self.0 & 0x02000000 != 0
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MapFlags(pub(crate) u32);

impl MapFlags {
    pub fn is_shared(&self) -> bool {
        self.0 & 0x1 != 0
    }

    pub fn is_private(&self) -> bool {
        self.0 & 0x2 != 0
    }

    pub fn is_fixed(&self) -> bool {
        self.0 & 0x10 != 0
    }

    pub fn is_anonymous(&self) -> bool {
        self.0 & 0x20 != 0
    }

    pub fn is_uninitialized(&self) -> bool {
        self.0 & 0x4000000 != 0
    }
}

#[derive(Debug)]
pub struct CountAndSize {
    pub count: u64,
    pub size: u64,
}

#[inline]
fn binary_search_range<'a, T, V, W, F>(
    array: &'a [T],
    min: Option<V>,
    max: Option<V>,
    callback: F,
) -> Range<usize>
where
    V: Ord + 'a,
    W: Borrow<V> + 'a,
    F: Fn(&'a T) -> W,
{
    if array.is_empty() {
        return 0..0;
    }

    let start = if let Some(min) = min {
        if min > *callback(array.last().unwrap()).borrow() {
            return 0..0;
        }

        if min <= *callback(array.first().unwrap()).borrow() {
            0
        } else {
            match array.binary_search_by(|key| callback(key).borrow().cmp(&min)) {
                Ok(mut index) => {
                    while index > 0
                        && callback(&array[index - 1]).borrow().cmp(&min) == Ordering::Equal
                    {
                        index -= 1;
                    }
                    index
                }
                Err(index) => index,
            }
        }
    } else {
        0
    };

    let end = if let Some(max) = max {
        if max < *callback(array.first().unwrap()).borrow() {
            return 0..0;
        }

        if max >= *callback(array.last().unwrap()).borrow() {
            array.len()
        } else {
            match array.binary_search_by(|key| callback(key).borrow().cmp(&max)) {
                Ok(mut index) => {
                    while index + 1 < array.len()
                        && callback(&array[index + 1]).borrow().cmp(&max) == Ordering::Equal
                    {
                        index += 1;
                    }
                    index + 1
                }
                Err(index) => index,
            }
        }
    } else {
        array.len()
    };

    start..end
}

#[cfg(test)]
mod tests {
    use super::binary_search_range;
    use std::ops::Range;

    quickcheck! {
        fn binary_search_range_works( xs: Vec< u8 >, min: Option< u8 >, max: Option< u8 > ) -> bool {
            let mut xs = xs;
            let mut min = min;
            let mut max = max;
            let swap = match (min, max) {
                (Some( min ), Some( max )) if max < min => true,
                _ => false
            };

            if swap {
                ::std::mem::swap( &mut min, &mut max );
            }

            xs.sort();
            let range = binary_search_range( &xs, min, max, |value| value );
            let expected: Vec< _ > = xs.iter().cloned().filter( |&x| min.map( |min| x >= min ).unwrap_or( true ) && max.map( |max| x <= max ).unwrap_or( true ) ).collect();
            &xs[ range ] == expected.as_slice()
        }
    }

    #[test]
    fn test_binary_search_range() {
        assert_eq!(
            binary_search_range(&[86, 87][..], None, Some(86), |value| value),
            0..1
        );

        assert_eq!(
            binary_search_range(&[0, 80, 80][..], Some(80), None, |value| value),
            1..3
        );

        assert_eq!(
            binary_search_range(
                &[0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2][..],
                Some(1),
                Some(1),
                |value| value
            ),
            1..10
        );
    }
}

pub trait SliceLikeIterator: DoubleEndedIterator + ExactSizeIterator + FusedIterator {}

impl<T> SliceLikeIterator for T where T: DoubleEndedIterator + ExactSizeIterator + FusedIterator {}

impl Data {
    #[inline]
    pub fn interner(&self) -> &StringInterner {
        &self.interner
    }

    #[inline]
    pub fn unsorted_allocations(&self) -> impl SliceLikeIterator<Item = &Allocation> {
        self.allocations.iter()
    }

    #[inline]
    fn sorted_by<'a, T: Ord + 'a, F: Fn(&'a Allocation) -> &'a T>(
        &'a self,
        array: &'a [AllocationId],
        min: Option<T>,
        max: Option<T>,
        callback: F,
    ) -> impl SliceLikeIterator<Item = (AllocationId, &'a Allocation)> {
        let range = binary_search_range(array, min, max, move |&id| {
            callback(&self.allocations[id.raw() as usize])
        });
        array[range]
            .iter()
            .map(move |&id| (id, &self.allocations[id.raw() as usize]))
    }

    #[inline]
    pub fn alloc_sorted_by_timestamp(
        &self,
        min: Option<Timestamp>,
        max: Option<Timestamp>,
    ) -> impl SliceLikeIterator<Item = (AllocationId, &Allocation)> {
        self.sorted_by(&self.sorted_by_timestamp, min, max, |alloc| {
            &alloc.timestamp
        })
    }

    #[inline]
    pub fn alloc_sorted_by_size(
        &self,
        min: Option<u64>,
        max: Option<u64>,
    ) -> impl SliceLikeIterator<Item = (AllocationId, &Allocation)> {
        self.sorted_by(&self.sorted_by_size, min, max, |alloc| &alloc.size)
    }

    #[inline]
    pub fn alloc_sorted_by_address(
        &self,
        min: Option<u64>,
        max: Option<u64>,
    ) -> impl SliceLikeIterator<Item = (AllocationId, &Allocation)> {
        self.sorted_by(&self.sorted_by_address, min, max, |alloc| &alloc.pointer)
    }

    #[inline]
    pub fn allocations_with_id(&self) -> impl Iterator<Item = (AllocationId, &Allocation)> {
        self.allocations
            .iter()
            .enumerate()
            .map(|(index, allocation)| (AllocationId::new(index as _), allocation))
    }

    #[inline]
    pub fn operations<'a>(&'a self) -> impl Iterator<Item = Operation<'a>> + 'a {
        self.operations.iter().map(move |op| {
            let allocation_id = op.id();
            let allocation = &self.allocations[allocation_id.raw() as usize];
            if op.is_allocation() {
                Operation::Allocation {
                    allocation,
                    allocation_id,
                }
            } else if op.is_reallocation() {
                let previous_allocation =
                    &self.allocations[allocation.reallocated_from.unwrap().raw() as usize];
                Operation::Reallocation {
                    new_allocation: allocation,
                    deallocation: previous_allocation.deallocation.as_ref().unwrap(),
                    old_allocation: previous_allocation,
                }
            } else {
                Operation::Deallocation {
                    allocation,
                    allocation_id,
                    deallocation: allocation.deallocation.as_ref().unwrap(),
                }
            }
        })
    }

    pub fn total_allocated(&self) -> u64 {
        self.total_allocated
    }

    pub fn total_allocated_count(&self) -> u64 {
        self.total_allocated_count
    }

    pub fn total_freed(&self) -> u64 {
        self.total_freed
    }

    pub fn total_freed_count(&self) -> u64 {
        self.total_freed_count
    }

    #[inline]
    pub fn initial_timestamp(&self) -> Timestamp {
        self.initial_timestamp
    }

    #[inline]
    pub fn last_timestamp(&self) -> Timestamp {
        self.last_timestamp
    }

    #[inline]
    pub fn pointer_size(&self) -> u64 {
        self.pointer_size
    }

    #[inline]
    pub fn executable(&self) -> &str {
        &self.executable
    }

    #[inline]
    pub fn architecture(&self) -> &str {
        &self.architecture
    }

    #[inline]
    pub fn id(&self) -> DataId {
        self.id
    }

    #[inline]
    pub fn unique_backtrace_count(&self) -> usize {
        self.backtraces.len()
    }

    #[inline]
    pub fn maximum_backtrace_depth(&self) -> u32 {
        self.maximum_backtrace_depth
    }

    #[inline]
    pub fn get_frame_ids(&self, id: BacktraceId) -> &[FrameId] {
        let (offset, length) = self.backtraces[id.0 as usize];
        &self.backtraces_storage[(offset as usize)..(offset + length) as usize]
    }

    pub fn get_frame(&self, id: FrameId) -> &Frame {
        &self.frames[id]
    }

    pub fn get_allocation(&self, id: AllocationId) -> &Allocation {
        &self.allocations[id.raw() as usize]
    }

    pub fn get_allocations_by_backtrace(
        &self,
        id: BacktraceId,
    ) -> impl SliceLikeIterator<Item = (AllocationId, &Allocation)> {
        self.allocations_by_backtrace
            .get(id.raw() as _)
            .iter()
            .map(move |&allocation_id| {
                (
                    allocation_id,
                    &self.allocations[allocation_id.raw() as usize],
                )
            })
    }

    pub fn get_allocation_ids_by_backtrace(&self, id: BacktraceId) -> &[AllocationId] {
        self.allocations_by_backtrace.get(id.raw() as _)
    }

    pub fn get_backtrace<'a>(
        &'a self,
        id: BacktraceId,
    ) -> impl SliceLikeIterator<Item = (FrameId, &'a Frame)> + Clone {
        self.get_frame_ids(id)
            .iter()
            .rev()
            .map(move |&frame_id| (frame_id, &self.frames[frame_id]))
    }

    pub fn get_group_statistics(&self, id: BacktraceId) -> &GroupStatistics {
        &self.group_stats[id.raw() as usize]
    }

    pub fn all_backtraces<'a>(
        &'a self,
    ) -> impl SliceLikeIterator<Item = (BacktraceId, impl Iterator<Item = (FrameId, &'a Frame)>)>
    {
        (0..self.backtraces.len()).into_iter().map(move |id| {
            let id = BacktraceId::new(id as _);
            (id, self.get_backtrace(id))
        })
    }

    pub fn get_non_inline_backtrace<'a>(
        &'a self,
        id: BacktraceId,
    ) -> impl Iterator<Item = (FrameId, &'a Frame)> + FusedIterator + DoubleEndedIterator {
        let mut last_address = None;
        self.get_backtrace(id).filter_map(move |(frame_id, frame)| {
            let ok = last_address
                .map(|last_address| last_address != frame.address())
                .unwrap_or(true);
            last_address = Some(frame.address());

            if ok {
                Some((frame_id, frame))
            } else {
                None
            }
        })
    }

    pub fn raw_tree(&self) -> Tree<CodePointer, FrameId> {
        let mut tree = Tree::new();
        for (allocation_id, allocation) in self.allocations_with_id() {
            if allocation.was_deallocated() {
                continue;
            }

            tree.add_allocation(
                &allocation,
                allocation_id,
                self.get_non_inline_backtrace(allocation.backtrace)
                    .map(|(frame_id, frame)| (frame.address(), frame_id)),
            );
        }

        tree
    }

    pub fn tree_by_source<F>(&self, filter: F) -> Tree<SourceKey, FrameId>
    where
        F: Fn(&Allocation) -> bool,
    {
        let mut tree = Tree::new();
        for (allocation_id, allocation) in self.allocations_with_id() {
            if !filter(allocation) {
                continue;
            }

            tree.add_allocation(
                &allocation,
                allocation_id,
                self.get_backtrace(allocation.backtrace)
                    .map(|(frame_id, frame)| {
                        let key = match (
                            frame.source(),
                            frame.line(),
                            frame.function().or(frame.raw_function()),
                        ) {
                            (Some(source), Some(line), _) => SourceKey::Location(source, line),
                            (_, _, Some(function)) => SourceKey::Function(function),
                            _ => SourceKey::Address(frame.address()),
                        };

                        (key, frame_id)
                    }),
            );
        }

        tree
    }

    pub fn dump_tree(&self, tree: &Tree<SourceKey, FrameId>) -> Vec<Vec<String>> {
        dump_tree(&tree, self.initial_timestamp, |&frame_id| {
            let frame = &self.frames[frame_id];
            if let Some(function) = frame.any_function() {
                let function = self.interner.resolve(function).unwrap();
                if let (Some(source), Some(line)) = (frame.source(), frame.line()) {
                    let source = self.interner.resolve(source).unwrap();
                    let filename = &source[source.rfind("/").map(|index| index + 1).unwrap_or(0)..];
                    format!("{} [{}:{}]", function, filename, line)
                } else {
                    format!("{}", function)
                }
            } else if let Some(library) = frame.library() {
                format!(
                    "{} [{}]",
                    frame.address(),
                    self.interner.resolve(library).unwrap()
                )
            } else {
                format!("{}", frame.address())
            }
        })
    }

    pub fn mallopts(&self) -> &[Mallopt] {
        &self.mallopts
    }

    pub fn mmap_operations(&self) -> &[MmapOperation] {
        &self.mmap_operations
    }

    pub fn get_dynamic_constants(&self) -> BTreeMap<String, BTreeMap<u32, CountAndSize>> {
        self.collate_allocations(|frame| {
            let raw_function = match frame.raw_function() {
                Some(raw_function) => raw_function,
                None => return false,
            };

            let raw_function = self.interner().resolve(raw_function).unwrap();
            raw_function.contains("__static_initialization_and_destruction_0")
                && raw_function.contains(".constprop")
        })
    }

    pub fn get_dynamic_constants_ascii_tree(&self) -> String {
        let constants = self.get_dynamic_constants();
        self.collation_to_ascii_tree(constants)
    }

    pub fn get_dynamic_statics(&self) -> BTreeMap<String, BTreeMap<u32, CountAndSize>> {
        self.collate_allocations(|frame| {
            let raw_function = match frame.raw_function() {
                Some(raw_function) => raw_function,
                None => return false,
            };

            let raw_function = self.interner().resolve(raw_function).unwrap();
            raw_function.contains("__static_initialization_and_destruction_0")
                && !raw_function.contains(".constprop")
        })
    }

    pub fn get_dynamic_statics_ascii_tree(&self) -> String {
        let constants = self.get_dynamic_statics();
        self.collation_to_ascii_tree(constants)
    }

    fn collate_allocations<F>(&self, filter: F) -> BTreeMap<String, BTreeMap<u32, CountAndSize>>
    where
        F: Fn(&Frame) -> bool,
    {
        let mut backtrace_to_src = HashMap::new();
        for (backtrace_id, frames) in self.all_backtraces() {
            for (_, frame) in frames {
                if !filter(frame) {
                    continue;
                }

                let source = match frame.source() {
                    Some(source) => source,
                    None => continue,
                };

                let line = match frame.line() {
                    Some(line) => line,
                    None => continue,
                };

                let mut source: Cow<str> = self.interner().resolve(source).unwrap().into();

                // We need this so that we can properly collapse the paths to the same file.
                const DISTCC_PATTERN: &'static str = "/distccd_";
                if let Some(index) = source.find(DISTCC_PATTERN) {
                    if source[index..]
                        .chars()
                        .skip(DISTCC_PATTERN.len() + 6)
                        .next()
                        == Some('/')
                    {
                        source = format!(
                            "{}/distccd_XXXXXX{}",
                            &source[..index],
                            &source[index + DISTCC_PATTERN.len() + 6..]
                        )
                        .into();
                    }
                }

                const DISTCC_STANDARD_PREFIX: &'static str = "/dev/shm/distcc/distccd_XXXXXX";
                if source.starts_with(DISTCC_STANDARD_PREFIX) {
                    source = format!("{}", &source[DISTCC_STANDARD_PREFIX.len()..]).into();
                }

                backtrace_to_src.insert(backtrace_id, (source, line));
                break;
            }
        }

        let mut per_file = HashMap::new();
        for allocation in self.unsorted_allocations() {
            if allocation.was_deallocated() {
                continue;
            }

            let src = match backtrace_to_src.get(&allocation.backtrace).cloned() {
                Some(src) => src,
                None => continue,
            };

            let (source, line) = src;
            let per_line = per_file.entry(source).or_insert_with(|| BTreeMap::new());
            let stats = per_line
                .entry(line)
                .or_insert(CountAndSize { count: 0, size: 0 });
            stats.count += 1;
            stats.size += allocation.usable_size();
        }

        per_file
            .into_iter()
            .map(|(key_id, value)| {
                let key = key_id.into_owned();
                (key, value)
            })
            .collect()
    }

    fn collation_to_ascii_tree(
        &self,
        collation: BTreeMap<String, BTreeMap<u32, CountAndSize>>,
    ) -> String {
        let mut total_count = 0;
        let mut total_size = 0;
        let mut row_count = 0;
        let mut collation: Vec<_> = collation
            .into_iter()
            .map(|(source, per_line)| {
                let mut whole_file_count = 0;
                let mut whole_file_size = 0;
                for (_, entry) in &per_line {
                    whole_file_count += entry.count;
                    whole_file_size += entry.size;
                }
                total_count += whole_file_count;
                total_size += whole_file_size;
                row_count = per_line.len() + 1;
                (whole_file_size, whole_file_count, source, per_line)
            })
            .collect();

        collation.sort_by(|a, b| b.0.cmp(&a.0));

        let mut table = Vec::with_capacity(row_count);
        table.push(vec![
            "SIZE".to_owned(),
            "COUNT".to_owned(),
            "SOURCE".to_owned(),
        ]);

        table.push(vec![
            format!("{}", ReadableSize(total_size)),
            format!("{}", total_count),
            "▒".to_owned(),
        ]);

        let mut sorted_per_line = Vec::new();
        let tree_count = collation.len();
        for (index, (whole_file_size, whole_file_count, source, per_line)) in
            collation.into_iter().enumerate()
        {
            let is_last_per_file = index + 1 == tree_count;
            let filename = &source[source.rfind("/").map(|index| index + 1).unwrap_or(0)..];

            if per_line.len() == 1 {
                let line = per_line.into_iter().next().unwrap().0;
                table.push(vec![
                    format!("{}", ReadableSize(whole_file_size)),
                    format!("{}", whole_file_count),
                    format!(
                        "{}─ {}:{} [{}]",
                        if is_last_per_file { ' ' } else { '|' },
                        filename,
                        line,
                        source
                    ),
                ]);
                continue;
            }

            table.push(vec![
                format!("{}", ReadableSize(whole_file_size)),
                format!("{}", whole_file_count),
                format!(
                    "{}─ {} [{}]",
                    if is_last_per_file { '└' } else { '├' },
                    filename,
                    source
                ),
            ]);

            sorted_per_line.extend(per_line.into_iter());
            sorted_per_line.sort_by(|a, b| (b.1).size.cmp(&(a.1).size));

            let subtree_count = sorted_per_line.len();
            for (index, (line, entry)) in sorted_per_line.drain(..).enumerate() {
                let is_last_per_line = index + 1 == subtree_count;
                table.push(vec![
                    format!("{}", ReadableSize(entry.size)),
                    format!("{}", entry.count),
                    format!(
                        "{} {}─ {}:{}",
                        if is_last_per_file { ' ' } else { '|' },
                        if is_last_per_line { '└' } else { '├' },
                        filename,
                        line
                    ),
                ]);
            }
        }

        table_to_string(&table)
    }
}
