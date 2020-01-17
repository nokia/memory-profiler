use std::collections::HashMap;
use std::io;

use byteorder::{NativeEndian, WriteBytesExt};

use crate::data::{Allocation, BacktraceId, Data, FrameId, Operation};

#[derive(Default)]
struct Exporter {
    free_slots: Vec<usize>,
    slot_count: usize,
    slot_by_pointer: HashMap<u64, usize>,
    slot_by_index: Vec<usize>,
    used_frames: HashMap<FrameId, u64>,
}

impl Exporter {
    fn preprocess_alloc(&mut self, allocation: &Allocation) {
        let slot = if self.free_slots.is_empty() {
            let slot = self.slot_count;
            self.slot_count += 1;
            slot
        } else {
            self.free_slots.pop().unwrap()
        };

        self.slot_by_pointer.insert(allocation.pointer, slot);
        self.slot_by_index.push(slot);
    }

    fn preprocess_dealloc(&mut self, allocation: &Allocation) {
        let slot = self.slot_by_pointer.remove(&allocation.pointer).unwrap();
        self.free_slots.push(slot);
        self.slot_by_index.push(slot);
    }

    fn preprocess_realloc(&mut self, new_allocation: &Allocation, old_allocation: &Allocation) {
        let slot = self
            .slot_by_pointer
            .remove(&old_allocation.pointer)
            .unwrap();
        self.slot_by_pointer.insert(new_allocation.pointer, slot);
        self.slot_by_index.push(slot);
    }

    fn preprocess_backtrace(&mut self, data: &Data, backtrace: BacktraceId) {
        for (frame_id, _) in data.get_backtrace(backtrace) {
            *self.used_frames.entry(frame_id).or_insert(0) += 1;
        }
    }

    fn generate_traversal(
        frame_map: &HashMap<FrameId, usize>,
        last_backtrace: &mut Option<BacktraceId>,
        mut output: impl io::Write,
        data: &Data,
        backtrace_id: BacktraceId,
    ) -> io::Result<()> {
        let backtrace = data.get_backtrace(backtrace_id);
        let (last_len, common_len) = if let Some(last_backtrace_id) = *last_backtrace {
            let last_backtrace = data.get_backtrace(last_backtrace_id);
            let last_len = last_backtrace.len();
            let common_len = backtrace
                .clone()
                .zip(last_backtrace)
                .take_while(|((a, _), (b, _))| a == b)
                .count();
            (last_len, common_len)
        } else {
            (0, 0)
        };

        let go_up_count = last_len - common_len;
        for _ in 0..go_up_count {
            output.write_u64::<NativeEndian>(5)?;
            output.write_u64::<NativeEndian>(0)?;
            output.write_u64::<NativeEndian>(0)?;
            output.write_u64::<NativeEndian>(0)?;
        }

        for (frame_id, _) in backtrace.skip(common_len) {
            output.write_u64::<NativeEndian>(4)?;
            output.write_u64::<NativeEndian>(*frame_map.get(&frame_id).unwrap() as _)?;
            output.write_u64::<NativeEndian>(0)?;
            output.write_u64::<NativeEndian>(0)?;
        }

        *last_backtrace = Some(backtrace_id);
        Ok(())
    }

    fn generate_alloc<T: io::Write>(
        mut output: T,
        slot: usize,
        allocation: &Allocation,
    ) -> io::Result<()> {
        let timestamp = allocation.timestamp.as_usecs();
        output.write_u64::<NativeEndian>(1)?;
        output.write_u64::<NativeEndian>(slot as u64)?;
        output.write_u64::<NativeEndian>(timestamp as u64)?;
        output.write_u64::<NativeEndian>(allocation.size as u64)?;
        Ok(())
    }

    fn generate_dealloc<T: io::Write>(
        mut output: T,
        slot: usize,
        allocation: &Allocation,
    ) -> io::Result<()> {
        let timestamp = allocation.timestamp.as_usecs();
        output.write_u64::<NativeEndian>(2)?;
        output.write_u64::<NativeEndian>(slot as u64)?;
        output.write_u64::<NativeEndian>(timestamp as u64)?;
        output.write_u64::<NativeEndian>(0)?;
        Ok(())
    }

    fn generate_realloc<T: io::Write>(
        mut output: T,
        slot: usize,
        new_allocation: &Allocation,
    ) -> io::Result<()> {
        let timestamp = new_allocation.timestamp.as_usecs();
        output.write_u64::<NativeEndian>(3)?;
        output.write_u64::<NativeEndian>(slot as u64)?;
        output.write_u64::<NativeEndian>(timestamp as u64)?;
        output.write_u64::<NativeEndian>(new_allocation.size as u64)?;
        Ok(())
    }

    fn process<T: io::Write, F: Fn(&Allocation) -> bool>(
        mut self,
        data: &Data,
        filter: F,
        mut output: T,
    ) -> io::Result<()> {
        for operation in data.operations() {
            match operation {
                Operation::Allocation { allocation, .. } => {
                    if !filter(allocation) {
                        continue;
                    }

                    self.preprocess_backtrace(data, allocation.backtrace);
                    self.preprocess_alloc(allocation);
                }
                Operation::Deallocation {
                    allocation,
                    deallocation,
                    ..
                } => {
                    if !filter(allocation) {
                        continue;
                    }

                    if let Some(backtrace) = deallocation.backtrace {
                        self.preprocess_backtrace(data, backtrace);
                    }
                    self.preprocess_dealloc(allocation);
                }
                Operation::Reallocation {
                    new_allocation,
                    old_allocation,
                    ..
                } => {
                    let is_new_ok = filter(new_allocation);
                    let is_old_ok = filter(old_allocation);

                    if is_new_ok || is_old_ok {
                        self.preprocess_backtrace(data, new_allocation.backtrace);
                    }

                    if is_new_ok && is_old_ok {
                        self.preprocess_realloc(new_allocation, old_allocation);
                    } else if is_new_ok {
                        self.preprocess_alloc(new_allocation);
                    } else if is_old_ok {
                        self.preprocess_dealloc(old_allocation);
                    }
                }
            }
        }

        output.write_u64::<NativeEndian>(self.slot_count as u64)?;
        let mut frames: Vec<_> = self.used_frames.drain().collect();
        frames.sort_by_key(|&(_, count)| count);
        frames.reverse();
        let frame_map: HashMap<_, _> = frames
            .into_iter()
            .enumerate()
            .map(|(index, (frame_id, _))| (frame_id, index))
            .collect();

        let mut last_backtrace = None;
        for (operation, slot) in data.operations().zip(self.slot_by_index) {
            match operation {
                Operation::Allocation { allocation, .. } => {
                    if !filter(allocation) {
                        continue;
                    }

                    Self::generate_traversal(
                        &frame_map,
                        &mut last_backtrace,
                        &mut output,
                        data,
                        allocation.backtrace,
                    )?;
                    Self::generate_alloc(&mut output, slot, allocation)?;
                }
                Operation::Deallocation {
                    allocation,
                    deallocation,
                    ..
                } => {
                    if !filter(allocation) {
                        continue;
                    }

                    if let Some(backtrace) = deallocation.backtrace {
                        Self::generate_traversal(
                            &frame_map,
                            &mut last_backtrace,
                            &mut output,
                            data,
                            backtrace,
                        )?;
                    }
                    Self::generate_dealloc(&mut output, slot, allocation)?;
                }
                Operation::Reallocation {
                    new_allocation,
                    old_allocation,
                    ..
                } => {
                    let is_new_ok = filter(new_allocation);
                    let is_old_ok = filter(old_allocation);

                    if is_new_ok || is_old_ok {
                        Self::generate_traversal(
                            &frame_map,
                            &mut last_backtrace,
                            &mut output,
                            data,
                            new_allocation.backtrace,
                        )?;
                    }

                    if is_new_ok && is_old_ok {
                        Self::generate_realloc(&mut output, slot, new_allocation)?;
                    } else if is_new_ok {
                        Self::generate_alloc(&mut output, slot, new_allocation)?;
                    } else if is_old_ok {
                        Self::generate_dealloc(&mut output, slot, old_allocation)?;
                    }
                }
            }
        }

        output.write_u64::<NativeEndian>(0)?;
        Ok(())
    }
}

pub fn export_as_replay<T: io::Write, F: Fn(&Allocation) -> bool>(
    data: &Data,
    output: T,
    filter: F,
) -> io::Result<()> {
    Exporter::default().process(data, filter, output)
}
