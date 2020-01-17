use crate::data::Timestamp;
use crate::tree::{NodeId, Tree};
use crate::util::{ReadableDuration, ReadableSize};

fn dump_node<K: PartialEq + Clone, V, F: Fn(&V) -> String>(
    tree: &Tree<K, V>,
    initial_timestamp: Timestamp,
    printer: &mut F,
    node_id: NodeId,
    indentation: String,
    stack: &mut Vec<bool>,
    output: &mut Vec<Vec<String>>,
) {
    if node_id == 0 {
        output.push(vec![
            "SIZE".to_owned(),
            "COUNT".to_owned(),
            "FIRST".to_owned(),
            "LAST".to_owned(),
            "SOURCE".to_owned(),
        ]);
    }

    let mut line = Vec::new();
    let value = {
        let node = tree.get_node(node_id);
        if node.total_count == 0 {
            return;
        }

        line.push(format!("{}", ReadableSize(node.total_size)));
        line.push(format!("{}", node.total_count));
        line.push(format!(
            "{}",
            ReadableDuration((node.total_first_timestamp - initial_timestamp).as_secs())
        ));
        line.push(format!(
            "{}",
            ReadableDuration((node.total_last_timestamp - initial_timestamp).as_secs())
        ));
        node.value()
    };

    if let Some(value) = value {
        line.push(format!("{}{}", indentation, printer(value)));
    } else {
        line.push(format!("▒"));
    }

    output.push(line);

    let mut child_indentation = String::new();
    for &is_last in stack.iter() {
        if is_last {
            child_indentation.push_str(" ");
        } else {
            child_indentation.push_str("|");
        }
    }

    let children_count = tree.get_node(node_id).children.len();
    for (index, &(_, child_id)) in tree.get_node(node_id).children.iter().enumerate() {
        let mut next_indentation = child_indentation.clone();
        if index + 1 == children_count {
            next_indentation.push_str("└");
            stack.push(true);
        } else {
            next_indentation.push_str("├");
            stack.push(false);
        }

        dump_node(
            tree,
            initial_timestamp,
            printer,
            child_id,
            next_indentation,
            stack,
            output,
        );
        stack.pop();
    }
}

pub fn dump_tree<K: PartialEq + Clone, V, F: Fn(&V) -> String>(
    tree: &Tree<K, V>,
    initial_timestamp: Timestamp,
    mut printer: F,
) -> Vec<Vec<String>> {
    let mut stack = Vec::new();
    let mut output = Vec::new();
    dump_node(
        tree,
        initial_timestamp,
        &mut printer,
        0,
        "".to_owned(),
        &mut stack,
        &mut output,
    );
    output
}
