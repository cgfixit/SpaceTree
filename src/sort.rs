use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Node;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Modified,
    Created,
    Size,
    PercentOfDisk,
    Logical,
    Files,
}

/// Sort one sibling list in place.
pub fn sort_children(children: &mut [Node], column: SortColumn, descending: bool) {
    children.sort_by(|a, b| {
        let primary = match column {
            SortColumn::Name => name_ord(&a.name, &b.name),
            SortColumn::Modified => time_ord(a.modified, b.modified),
            SortColumn::Created => time_ord(a.created, b.created),
            SortColumn::Size => a.size.cmp(&b.size),
            SortColumn::PercentOfDisk => a
                .percent_of_disk
                .partial_cmp(&b.percent_of_disk)
                .unwrap_or(Ordering::Equal),
            SortColumn::Logical => a.logical.cmp(&b.logical),
            SortColumn::Files => a.files.cmp(&b.files),
        };
        let ord = if primary == Ordering::Equal {
            name_ord(&a.name, &b.name)
        } else {
            primary
        };
        if descending {
            ord.reverse()
        } else {
            ord
        }
    });
}

/// Recursively sort every sibling list in the tree.
pub fn sort_tree(node: &mut Node, column: SortColumn, descending: bool) {
    sort_children(&mut node.children, column, descending);
    for child in &mut node.children {
        sort_tree(child, column, descending);
    }
}

fn name_ord(a: &str, b: &str) -> Ordering {
    a.to_ascii_lowercase()
        .cmp(&b.to_ascii_lowercase())
        .then_with(|| a.cmp(b))
}

fn time_ord(a: Option<SystemTime>, b: Option<SystemTime>) -> Ordering {
    a.unwrap_or(UNIX_EPOCH).cmp(&b.unwrap_or(UNIX_EPOCH))
}
