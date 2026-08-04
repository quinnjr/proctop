use rtop::model::{Proc, ProcRow};
use rtop::sort::{SortKey, sort_procs};

fn row(pid: i32, name: &str, cpu: f32, rss: u64, cpu_time: u64) -> ProcRow {
    ProcRow {
        proc: Proc {
            pid,
            name: name.to_string(),
            utime: cpu_time,
            rss,
            ..Proc::default()
        },
        cpu,
        mem: 0.0,
        user: String::from("joseph"),
    }
}

fn pids(rows: &[ProcRow]) -> Vec<i32> {
    rows.iter().map(|r| r.proc.pid).collect()
}

#[test]
fn sorts_by_cpu_descending() {
    let mut rows = vec![
        row(1, "a", 0.1, 0, 0),
        row(2, "b", 0.9, 0, 0),
        row(3, "c", 0.5, 0, 0),
    ];

    sort_procs(&mut rows, SortKey::Cpu, true);

    assert_eq!(pids(&rows), vec![2, 3, 1]);
}

#[test]
fn sorts_by_cpu_ascending() {
    let mut rows = vec![
        row(1, "a", 0.1, 0, 0),
        row(2, "b", 0.9, 0, 0),
        row(3, "c", 0.5, 0, 0),
    ];

    sort_procs(&mut rows, SortKey::Cpu, false);

    assert_eq!(pids(&rows), vec![1, 3, 2]);
}

#[test]
fn sorts_by_memory() {
    let mut rows = vec![
        row(1, "a", 0.0, 100, 0),
        row(2, "b", 0.0, 300, 0),
        row(3, "c", 0.0, 200, 0),
    ];

    sort_procs(&mut rows, SortKey::Memory, true);

    assert_eq!(pids(&rows), vec![2, 3, 1]);
}

#[test]
fn sorts_by_name_case_insensitively() {
    // Otherwise every capitalized process sorts ahead of every lowercase
    // one, which reads as random to a user scanning the column.
    let mut rows = vec![
        row(1, "zsh", 0.0, 0, 0),
        row(2, "Xorg", 0.0, 0, 0),
        row(3, "apache", 0.0, 0, 0),
    ];

    sort_procs(&mut rows, SortKey::Name, false);

    assert_eq!(pids(&rows), vec![3, 2, 1]);
}

#[test]
fn sorts_by_accumulated_cpu_time() {
    let mut rows = vec![
        row(1, "a", 0.0, 0, 10),
        row(2, "b", 0.0, 0, 30),
        row(3, "c", 0.0, 0, 20),
    ];

    sort_procs(&mut rows, SortKey::Time, true);

    assert_eq!(pids(&rows), vec![2, 3, 1]);
}

#[test]
fn breaks_ties_by_pid_so_rows_do_not_jitter_between_frames() {
    // Most processes sit at 0% CPU. Without a deterministic tie-break the
    // idle bulk of the table reshuffles on every refresh, which makes the
    // list unreadable and unclickable.
    let mut rows = vec![
        row(30, "a", 0.0, 0, 0),
        row(10, "b", 0.0, 0, 0),
        row(20, "c", 0.0, 0, 0),
    ];

    sort_procs(&mut rows, SortKey::Cpu, true);

    assert_eq!(pids(&rows), vec![10, 20, 30]);
}

#[test]
fn breaks_ties_by_pid_ascending_regardless_of_sort_direction() {
    // The tie-break is about stability, not about the column's direction,
    // so it must not flip when the user reverses the sort.
    let mut rows = vec![
        row(30, "a", 0.0, 0, 0),
        row(10, "b", 0.0, 0, 0),
        row(20, "c", 0.0, 0, 0),
    ];

    sort_procs(&mut rows, SortKey::Cpu, false);

    assert_eq!(pids(&rows), vec![10, 20, 30]);
}

#[test]
fn orders_nan_cpu_last_instead_of_corrupting_the_sort() {
    // A NaN comparator returns None from partial_cmp; feeding that to
    // sort_by with a naive unwrap panics, and returning Equal silently
    // produces a scrambled list.
    let mut rows = vec![
        row(1, "a", f32::NAN, 0, 0),
        row(2, "b", 0.5, 0, 0),
        row(3, "c", 0.1, 0, 0),
    ];

    sort_procs(&mut rows, SortKey::Cpu, true);

    assert_eq!(pids(&rows), vec![2, 3, 1]);
}
