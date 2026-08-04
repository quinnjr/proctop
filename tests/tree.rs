use proctop::model::{Proc, ProcRow};
use proctop::tree::flatten;

fn row(pid: i32, ppid: i32, name: &str) -> ProcRow {
    ProcRow {
        proc: Proc {
            pid,
            ppid,
            name: name.to_string(),
            ..Proc::default()
        },
        ..ProcRow::default()
    }
}

fn shape(rows: &[ProcRow]) -> Vec<(i32, usize)> {
    rows.iter().map(|r| (r.proc.pid, r.depth)).collect()
}

#[test]
fn nests_children_under_their_parent() {
    let rows = vec![
        row(1, 0, "init"),
        row(2, 1, "child"),
        row(3, 2, "grandchild"),
    ];

    assert_eq!(shape(&flatten(rows)), vec![(1, 0), (2, 1), (3, 2)]);
}

#[test]
fn orders_a_parent_before_its_children_regardless_of_input_order() {
    let rows = vec![
        row(3, 2, "grandchild"),
        row(1, 0, "init"),
        row(2, 1, "child"),
    ];

    assert_eq!(shape(&flatten(rows)), vec![(1, 0), (2, 1), (3, 2)]);
}

#[test]
fn keeps_siblings_in_the_order_they_arrived() {
    // The caller has already sorted the list; the tree must not undo that
    // within a set of siblings.
    let rows = vec![
        row(1, 0, "init"),
        row(30, 1, "c"),
        row(10, 1, "a"),
        row(20, 1, "b"),
    ];

    assert_eq!(
        shape(&flatten(rows)),
        vec![(1, 0), (30, 1), (10, 1), (20, 1)]
    );
}

#[test]
fn treats_a_process_whose_parent_is_missing_as_a_root() {
    // Filtering can remove a parent while keeping its children, and a
    // process reparented to init mid-sample can reference a ppid that is no
    // longer in the list.
    let rows = vec![row(5, 999, "orphan"), row(6, 5, "child")];

    assert_eq!(shape(&flatten(rows)), vec![(5, 0), (6, 1)]);
}

#[test]
fn loses_no_processes() {
    let rows = vec![
        row(1, 0, "init"),
        row(2, 1, "a"),
        row(3, 1, "b"),
        row(4, 3, "c"),
        row(9, 404, "orphan"),
    ];
    let count = rows.len();

    assert_eq!(flatten(rows).len(), count);
}

#[test]
fn survives_a_parent_cycle_without_hanging() {
    // Should be impossible from a real /proc, but a torn read across two
    // sample passes can produce one, and an infinite loop in a monitor is
    // worse than a strange-looking tree.
    let rows = vec![row(1, 2, "a"), row(2, 1, "b")];

    let flattened = flatten(rows);

    assert_eq!(flattened.len(), 2);
}

#[test]
fn handles_an_empty_list() {
    assert!(flatten(Vec::new()).is_empty());
}

#[test]
fn handles_a_process_that_is_its_own_parent() {
    // pid 1's ppid is 0, but a torn read can briefly show self-parenting.
    let rows = vec![row(1, 1, "weird")];

    assert_eq!(shape(&flatten(rows)), vec![(1, 0)]);
}
