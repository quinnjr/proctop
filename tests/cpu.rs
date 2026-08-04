use rtop::sample::cpu;

const PROC_STAT: &str = include_str!("fixtures/proc_stat.txt");

#[test]
fn parses_aggregate_cpu_times_from_proc_stat() {
    let stat = cpu::parse_stat(PROC_STAT).expect("fixture should parse");

    // First line of the fixture:
    // cpu  191641 5197 59589 893149 257938 2941 1882 0 0 0
    assert_eq!(stat.total.user, 191_641);
    assert_eq!(stat.total.nice, 5_197);
    assert_eq!(stat.total.system, 59_589);
    assert_eq!(stat.total.idle, 893_149);
    assert_eq!(stat.total.iowait, 257_938);
    assert_eq!(stat.total.irq, 2_941);
    assert_eq!(stat.total.softirq, 1_882);
    assert_eq!(stat.total.steal, 0);
}

#[test]
fn parses_one_entry_per_core_in_kernel_order() {
    let stat = cpu::parse_stat(PROC_STAT).expect("fixture should parse");

    assert_eq!(
        stat.cores.len(),
        32,
        "fixture was captured on a 32-core box"
    );

    // cpu0 10398 633 2322 24773 5029 316 481 0 0 0
    assert_eq!(stat.cores[0].user, 10_398);
    assert_eq!(stat.cores[0].idle, 24_773);

    // cpu31 is the last line, and must not be confused with the aggregate.
    assert_ne!(stat.cores[31], stat.total);
}

#[test]
fn ignores_the_non_cpu_lines_that_follow() {
    // `intr`, `ctxt`, `btime`, `softirq` and friends sit below the cpu lines
    // and must not be mistaken for cores.
    let stat = cpu::parse_stat(PROC_STAT).expect("fixture should parse");

    assert_eq!(stat.cores.len(), 32);
}

#[test]
fn returns_none_when_the_aggregate_line_is_missing() {
    assert!(cpu::parse_stat("ctxt 12345\nbtime 999\n").is_none());
}

#[test]
fn tolerates_a_kernel_that_stops_before_the_guest_columns() {
    // Pre-2.6.24 kernels emit seven columns, not ten. The missing ones read
    // as zero rather than failing the whole file.
    let stat = cpu::parse_stat("cpu  1 2 3 4 5 6 7\n").expect("should parse");

    assert_eq!(stat.total.softirq, 7);
    assert_eq!(stat.total.steal, 0);
    assert_eq!(stat.total.guest_nice, 0);
}
