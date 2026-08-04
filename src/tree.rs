//! Parent/child nesting for the tree view.

use std::collections::{HashMap, HashSet};

use crate::model::ProcRow;

/// Reorder `rows` so each process follows its parent, recording nesting
/// depth on each row.
///
/// Sibling order is preserved, because the caller has already sorted the
/// list and the tree must not undo that within a set of siblings.
///
/// A process whose parent is absent becomes a root. That is not an error
/// case: filtering can remove a parent while keeping its children, and a
/// process reparented mid-sample can name a ppid no longer in the list.
pub fn flatten(rows: Vec<ProcRow>) -> Vec<ProcRow> {
    let present: HashSet<i32> = rows.iter().map(|r| r.proc.pid).collect();

    let mut children: HashMap<i32, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();

    for (i, row) in rows.iter().enumerate() {
        let ppid = row.proc.ppid;
        // Self-parenting would make a process its own ancestor and vanish
        // from the output entirely.
        if ppid != row.proc.pid && present.contains(&ppid) {
            children.entry(ppid).or_default().push(i);
        } else {
            roots.push(i);
        }
    }

    let mut out = Vec::with_capacity(rows.len());
    let mut emitted = vec![false; rows.len()];
    // Depth-first, pushing siblings in reverse so they pop in order.
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|&i| (i, 0)).collect();

    while let Some((i, depth)) = stack.pop() {
        if emitted[i] {
            continue;
        }
        emitted[i] = true;
        out.push((i, depth));

        if let Some(kids) = children.get(&rows[i].proc.pid) {
            stack.extend(kids.iter().rev().map(|&k| (k, depth + 1)));
        }
    }

    // A parent cycle leaves its members unreachable from any root. That
    // should be impossible from a real /proc, but a torn read across two
    // passes can produce one, and silently dropping processes is worse than
    // showing a strange tree — as is looping forever.
    for (i, done) in emitted.iter().enumerate() {
        if !done {
            out.push((i, 0));
        }
    }

    let mut rows: Vec<Option<ProcRow>> = rows.into_iter().map(Some).collect();
    out.into_iter()
        .map(|(i, depth)| {
            let mut row = rows[i].take().expect("each index is emitted once");
            row.depth = depth;
            row
        })
        .collect()
}
