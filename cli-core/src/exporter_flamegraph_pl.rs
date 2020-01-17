use super::{Allocation, Data, Frame, FrameId, NodeId, Tree};

use std::fmt::{self, Write};

fn dump_collation_impl<O: FnMut(&str) -> Result<(), E>, K: PartialEq + Clone, E>(
    data: &Data,
    tree: &Tree<K, &Frame>,
    node_id: NodeId,
    stack: &mut Vec<String>,
    cache: &mut Vec<String>,
    output: &mut O,
) -> Result<(), E> {
    let node = tree.get_node(node_id);

    if let Some(value) = node.value() {
        let mut buffer = cache.pop().unwrap_or(String::new());
        let library = value
            .library()
            .map(|id| data.interner().resolve(id).unwrap())
            .unwrap_or("???");
        if let Some(function) = value
            .function()
            .map(|id| data.interner().resolve(id).unwrap())
        {
            write!(&mut buffer, "{} [{}]", function, library).unwrap();
        } else if let Some(function) = value
            .raw_function()
            .map(|id| data.interner().resolve(id).unwrap())
        {
            write!(&mut buffer, "{} [{}]", function, library).unwrap();
        } else {
            write!(
                &mut buffer,
                "0x{:016X} [{}]",
                value.address().raw(),
                library
            )
            .unwrap();
        }
        stack.push(buffer);
    }

    if node.self_count != 0 {
        let mut buffer = cache.pop().unwrap_or(String::new());
        write!(&mut buffer, "{} {}", stack.join(";"), node.self_size).unwrap();

        output(&buffer)?;

        buffer.clear();
        cache.push(buffer);
    }

    for &(_, child_id) in tree.get_node(node_id).children.iter() {
        dump_collation_impl(data, tree, child_id, stack, cache, output)?;
    }

    if !node.is_root() {
        let mut buffer = stack.pop().unwrap();
        buffer.clear();
        cache.push(buffer);
    }

    Ok(())
}

pub fn dump_collation<F, O, E>(data: &Data, filter: F, mut output: O) -> Result<(), E>
where
    F: Fn(&Allocation) -> bool,
    O: FnMut(&str) -> Result<(), E>,
{
    let mut tree: Tree<FrameId, &Frame> = Tree::new();
    for (allocation_id, allocation) in data.allocations_with_id() {
        if !filter(allocation) {
            continue;
        }

        tree.add_allocation(
            allocation,
            allocation_id,
            data.get_backtrace(allocation.backtrace),
        );
    }

    dump_collation_impl(
        data,
        &tree,
        0,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut output,
    )
}

pub fn export_as_flamegraph_pl<T: fmt::Write, F: Fn(&Allocation) -> bool>(
    data: &Data,
    mut output: T,
    filter: F,
) -> fmt::Result {
    dump_collation(data, filter, |line| writeln!(&mut output, "{}", line))
}
