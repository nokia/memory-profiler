use std::cmp::{max, min};
use std::collections::HashMap;
use std::mem::{self, ManuallyDrop};

use crate::data::{Allocation, AllocationId, DataPointer, Timestamp};

pub type NodeId = u64;

pub struct Node<K, V> {
    key: ManuallyDrop<K>,
    value: ManuallyDrop<V>,
    pub total_size: u64,
    pub total_count: u64,
    pub total_first_timestamp: Timestamp,
    pub total_last_timestamp: Timestamp,
    pub self_size: u64,
    pub self_count: u64,
    pub self_allocations: Vec<AllocationId>,
    pub children: Vec<(K, NodeId)>,
    pub parent: NodeId,
}

impl<K, V> Node<K, V> {
    #[inline]
    pub fn value(&self) -> Option<&V> {
        if self.is_root() {
            None
        } else {
            Some(&*self.value)
        }
    }

    #[inline]
    pub fn is_root(&self) -> bool {
        self.parent == -1_i64 as NodeId
    }
}

impl<K, V> Drop for Node<K, V> {
    fn drop(&mut self) {
        if !self.is_root() {
            unsafe {
                ManuallyDrop::drop(&mut self.key);
                ManuallyDrop::drop(&mut self.value);
            }
        }
    }
}

pub struct Tree<K, V> {
    allocations: HashMap<DataPointer, (NodeId, usize)>,
    nodes: Vec<Node<K, V>>,
}

impl<K, V> Tree<K, V>
where
    K: PartialEq + Clone,
{
    #[inline(always)]
    pub fn new() -> Self {
        let root = Node {
            key: ManuallyDrop::new(unsafe { mem::uninitialized() }),
            value: ManuallyDrop::new(unsafe { mem::uninitialized() }),
            total_size: 0,
            total_count: 0,
            total_first_timestamp: Timestamp::max(),
            total_last_timestamp: Timestamp::min(),
            self_size: 0,
            self_count: 0,
            self_allocations: Vec::new(),
            children: Vec::new(),
            parent: -1_i64 as NodeId,
        };

        Tree {
            allocations: HashMap::new(),
            nodes: vec![root],
        }
    }

    #[inline]
    fn get_child_id(&self, node_id: NodeId, key: &K) -> Option<NodeId> {
        self.nodes[node_id as usize]
            .children
            .iter()
            .find(|&(child_key, _)| *child_key == *key)
            .map(|(_, child_id)| child_id)
            .cloned()
    }

    pub fn add_allocation<T>(
        &mut self,
        allocation: &Allocation,
        allocation_id: AllocationId,
        backtrace: T,
    ) where
        T: Iterator<Item = (K, V)>,
    {
        let timestamp = allocation.timestamp;
        let size = allocation.size;

        let mut node_id: NodeId = 0;
        for (key, value) in backtrace {
            {
                let node = &mut self.nodes[node_id as usize];
                node.total_size += size;
                node.total_count += 1;
                node.total_first_timestamp = min(node.total_first_timestamp, timestamp);
                node.total_last_timestamp = max(node.total_last_timestamp, timestamp);
            }
            let child_id = self.get_child_id(node_id, &key);
            let child_id = if child_id.is_none() {
                let child_node = Node {
                    key: ManuallyDrop::new(key.clone()),
                    value: ManuallyDrop::new(value),
                    total_size: 0,
                    total_count: 0,
                    total_first_timestamp: timestamp,
                    total_last_timestamp: timestamp,
                    self_size: 0,
                    self_count: 0,
                    self_allocations: Vec::new(),
                    children: Vec::new(),
                    parent: node_id,
                };

                let child_id = self.nodes.len() as NodeId;
                self.nodes.push(child_node);
                self.nodes[node_id as usize].children.push((key, child_id));
                child_id
            } else {
                child_id.unwrap()
            };

            node_id = child_id;
        }

        let node = &mut self.nodes[node_id as usize];
        node.self_size += size;
        node.self_count += 1;
        node.total_size += size;
        node.total_count += 1;
        node.total_first_timestamp = min(node.total_first_timestamp, timestamp);
        node.total_last_timestamp = max(node.total_last_timestamp, timestamp);

        let index = node.self_allocations.len();
        self.allocations
            .insert(allocation.pointer, (node_id, index));
        node.self_allocations.push(allocation_id);
    }

    pub fn currently_allocated(&self) -> u64 {
        self.nodes[0].total_size
    }

    pub fn currently_allocated_count(&self) -> u64 {
        self.nodes[0].total_count
    }

    pub fn get_node(&self, id: NodeId) -> &Node<K, V> {
        &self.nodes[id as usize]
    }
}
