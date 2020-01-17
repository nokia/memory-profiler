use std::collections::HashMap;
use std::fmt;
use std::io;

use super::{Allocation, BacktraceId, CodePointer, Data, Frame, Operation, StringId, Timestamp};

use crate::io_adapter::IoAdapter;

#[derive(PartialEq, Eq, Hash)]
struct AllocInfo {
    size: u64,
    backtrace: usize,
}

struct Child {
    ip_index: usize,
    trace_node_index: usize,
}

struct TraceNode {
    children: Vec<Child>,
}

pub struct HeaptrackExporter<'a, T: fmt::Write> {
    alloc_info_to_index: HashMap<AllocInfo, usize>,
    backtrace_to_index: HashMap<BacktraceId, usize>,
    ip_to_index: HashMap<CodePointer, usize>,
    string_map: HashMap<StringId, usize>,
    trace_tree: Vec<TraceNode>,
    tx: T,
    data: &'a Data,
    last_elapsed: Timestamp,
}

/*
    Format of the heaptrack data file:
      Header:
        v <heaptrack_version> <file_format_version>
        X <cmdline>
        I <page_size> <total_memory_in_pages>
      Allocation:
        + <alloc_info_index>
      Deallocation:
        - <alloc_info_index>
      Alloc info:
        a <size> <trace_index>
      Trace:
        t <ip_index> <parent_trace_index>
      IP:
        i <address> <module_name_index> [<function_name_index>] [<file_name_index> <line>] [for each inlined frame: <function_name_index> <file_name_index> <line>]...
      String:
        s <string>
*/

impl<'a, T: fmt::Write> HeaptrackExporter<'a, T> {
    fn new(data: &'a Data, mut tx: T) -> Result<Self, fmt::Error> {
        writeln!(tx, "v 10100 2")?;
        writeln!(tx, "X {}", data.executable())?;

        let exporter = HeaptrackExporter {
            alloc_info_to_index: HashMap::new(),
            backtrace_to_index: HashMap::new(),
            ip_to_index: HashMap::new(),
            string_map: HashMap::new(),
            trace_tree: vec![TraceNode {
                children: Vec::new(),
            }],
            tx,
            data,
            last_elapsed: Timestamp::min(),
        };

        Ok(exporter)
    }

    fn emit_timestamp(&mut self, timestamp: Timestamp) -> Result<(), fmt::Error> {
        let elapsed = timestamp - self.data.initial_timestamp();
        if self.last_elapsed != elapsed {
            writeln!(self.tx, "c {:x}", elapsed.as_msecs())?;
            self.last_elapsed = elapsed;
        }

        Ok(())
    }

    fn get_size(&self, allocation: &Allocation) -> u64 {
        allocation.size + allocation.extra_usable_space as u64
    }

    pub fn handle_alloc(&mut self, allocation: &Allocation) -> Result<(), fmt::Error> {
        let alloc_info = AllocInfo {
            size: self.get_size(allocation),
            backtrace: self.resolve_backtrace(allocation.backtrace)?,
        };

        self.emit_timestamp(allocation.timestamp)?;

        let alloc_info_index = self.resolve_alloc_info(alloc_info)?;
        writeln!(self.tx, "+ {:x}", alloc_info_index)
    }

    pub fn handle_dealloc(&mut self, allocation: &Allocation) -> Result<(), fmt::Error> {
        let alloc_info = AllocInfo {
            size: self.get_size(allocation),
            backtrace: self.resolve_backtrace(allocation.backtrace)?,
        };

        self.emit_timestamp(allocation.timestamp)?;

        let alloc_info_index = self.alloc_info_to_index.get(&alloc_info).unwrap();
        writeln!(self.tx, "- {:x}", alloc_info_index)
    }

    fn resolve_backtrace(&mut self, backtrace_id: BacktraceId) -> Result<usize, fmt::Error> {
        if let Some(&index) = self.backtrace_to_index.get(&backtrace_id) {
            return Ok(index);
        }

        let mut parent_trace_index = 0;
        let frame_ids = self.data.get_frame_ids(backtrace_id);
        if frame_ids.is_empty() {
            warn!("Empty backtrace with ID = {:?}", backtrace_id);
            return Ok(0);
        }

        let mut i = frame_ids.len() - 1;
        while i > 0 {
            let frame = self.data.get_frame(frame_ids[i]);
            if frame.is_inline() {
                i -= 1;
                continue;
            }

            i -= 1;

            let ip_index = self.resolve_ip(frame)?;
            if let Some(child) = self.trace_tree[parent_trace_index]
                .children
                .iter()
                .find(|child| child.ip_index == ip_index)
            {
                parent_trace_index = child.trace_node_index;
                continue;
            }

            let trace_node_index = self.trace_tree.len();
            self.trace_tree.push(TraceNode {
                children: Vec::new(),
            });

            self.trace_tree[parent_trace_index].children.push(Child {
                ip_index,
                trace_node_index,
            });

            writeln!(self.tx, "t {:x} {:x}", ip_index, parent_trace_index)?;
            parent_trace_index = trace_node_index;
        }

        self.backtrace_to_index
            .insert(backtrace_id, parent_trace_index);
        Ok(parent_trace_index)
    }

    fn resolve_ip(&mut self, frame: &Frame) -> Result<usize, fmt::Error> {
        let address = frame.address();
        if let Some(&index) = self.ip_to_index.get(&address) {
            return Ok(index);
        }

        let module_name_index;
        if let Some(library_id) = frame.library() {
            module_name_index = self.resolve_string(library_id)?;
        } else {
            module_name_index = 0;
        }

        let function_name_index;
        let source;

        if let Some(id) = frame.function().or(frame.raw_function()) {
            function_name_index = Some(self.resolve_string(id)?);
        } else {
            function_name_index = None;
        };

        match (frame.source(), frame.line()) {
            (Some(id), Some(line)) => {
                let index = self.resolve_string(id)?;
                source = Some((index, line));
            }
            _ => {
                source = None;
            }
        }

        write!(
            self.tx,
            "i {:x} {:x}",
            frame.address().raw(),
            module_name_index
        )?;
        if let Some(index) = function_name_index {
            write!(self.tx, " {:x}", index)?;

            if let Some((index, line)) = source {
                write!(self.tx, " {:x} {:x}", index, line)?;
            }
        }

        writeln!(self.tx, "")?;

        let index = self.ip_to_index.len() + 1;
        self.ip_to_index.insert(address, index);

        Ok(index)
    }

    fn resolve_string(&mut self, string_id: StringId) -> Result<usize, fmt::Error> {
        if let Some(&index) = self.string_map.get(&string_id) {
            return Ok(index);
        }

        writeln!(
            self.tx,
            "s {}",
            self.data.interner().resolve(string_id).unwrap()
        )?;

        let index = self.string_map.len() + 1;
        self.string_map.insert(string_id, index);
        Ok(index)
    }

    fn resolve_alloc_info(&mut self, alloc_info: AllocInfo) -> Result<usize, fmt::Error> {
        let alloc_info_index = self.alloc_info_to_index.get(&alloc_info).cloned();
        let alloc_info_index = match alloc_info_index {
            Some(value) => value,
            None => {
                writeln!(
                    self.tx,
                    "a {:x} {:x}",
                    alloc_info.size, alloc_info.backtrace
                )?;

                let index = self.alloc_info_to_index.len();
                self.alloc_info_to_index.insert(alloc_info, index);

                index
            }
        };

        Ok(alloc_info_index)
    }
}

fn io_err<T: fmt::Display>(err: T) -> io::Error {
    io::Error::new(
        io::ErrorKind::Other,
        format!("serialization failed: {}", err),
    )
}

pub fn export_as_heaptrack<T: io::Write, F: Fn(&Allocation) -> bool>(
    data: &Data,
    data_out: T,
    filter: F,
) -> io::Result<()> {
    let mut exporter = HeaptrackExporter::new(data, IoAdapter::new(data_out)).map_err(io_err)?;
    for op in data.operations() {
        match op {
            Operation::Allocation { allocation, .. } => {
                if !filter(allocation) {
                    continue;
                }

                exporter.handle_alloc(allocation).map_err(io_err)?;
            }
            Operation::Deallocation { allocation, .. } => {
                if !filter(allocation) {
                    continue;
                }

                exporter.handle_dealloc(allocation).map_err(io_err)?;
            }
            Operation::Reallocation {
                old_allocation,
                new_allocation,
                ..
            } => {
                if filter(old_allocation) {
                    exporter.handle_dealloc(old_allocation).map_err(io_err)?;
                }

                if filter(new_allocation) {
                    exporter.handle_alloc(new_allocation).map_err(io_err)?;
                }
            }
        }
    }

    Ok(())
}
