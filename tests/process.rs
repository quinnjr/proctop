use rtop::model::ProcState;
use rtop::sample::process;

const PID_STAT: &str = include_str!("fixtures/pid_stat.txt");
const PAGE: u64 = 4096;

#[test]
fn parses_the_fields_rtop_displays() {
    let p = process::parse_pid_stat(PID_STAT, PAGE).expect("fixture should parse");

    assert_eq!(p.pid, 43595);
    assert_eq!(p.name, "rust-analyzer");
    assert_eq!(p.state, ProcState::Sleeping);
    assert_eq!(p.ppid, 37139);
    assert_eq!(p.utime, 28_203);
    assert_eq!(p.stime, 1_583);
    assert_eq!(p.threads, 69);
    assert_eq!(p.priority, 20);
    assert_eq!(p.nice, 0);
    assert_eq!(p.starttime, 9_143);
}

#[test]
fn converts_rss_from_pages_to_bytes() {
    // `/proc/[pid]/stat` reports RSS in pages; reporting it raw would
    // understate every process by a factor of the page size.
    let p = process::parse_pid_stat(PID_STAT, PAGE).expect("fixture should parse");

    assert_eq!(p.rss, 1_328_058 * PAGE);
    assert_eq!(p.vsize, 7_067_922_432, "vsize is already in bytes");
}

#[test]
fn sums_user_and_system_time() {
    let p = process::parse_pid_stat(PID_STAT, PAGE).expect("fixture should parse");

    assert_eq!(p.cpu_time(), 28_203 + 1_583);
}

#[test]
fn handles_a_process_name_containing_spaces() {
    // Firefox does this: `(Isolated Web Co)`. Splitting on whitespace
    // shifts every later field.
    let line = "42 (Isolated Web Co) S 1 1 1 0 -1 0 0 0 0 0 7 3 0 0 20 0 4 0 999 4096 10 0\n";

    let p = process::parse_pid_stat(line, PAGE).expect("should parse");

    assert_eq!(p.name, "Isolated Web Co");
    assert_eq!(p.ppid, 1);
    assert_eq!(p.utime, 7);
    assert_eq!(p.stime, 3);
    assert_eq!(p.threads, 4);
}

#[test]
fn handles_a_process_name_containing_a_closing_paren() {
    // A process can name itself anything, including `foo)bar`. Scanning for
    // the FIRST `)` truncates the name and shifts every later field; the
    // last `)` is the only correct delimiter.
    let line = "42 (weird)name) R 1 1 1 0 -1 0 0 0 0 0 5 5 0 0 20 0 1 0 999 4096 10 0\n";

    let p = process::parse_pid_stat(line, PAGE).expect("should parse");

    assert_eq!(p.name, "weird)name");
    assert_eq!(p.state, ProcState::Running);
    assert_eq!(p.utime, 5);
}

#[test]
fn recognizes_every_state_the_kernel_reports() {
    for (ch, expected) in [
        ('R', ProcState::Running),
        ('S', ProcState::Sleeping),
        ('D', ProcState::UninterruptibleSleep),
        ('Z', ProcState::Zombie),
        ('T', ProcState::Stopped),
        ('t', ProcState::TracingStop),
        ('I', ProcState::Idle),
        ('X', ProcState::Dead),
    ] {
        let line = format!("1 (x) {ch} 0 0 0 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 0 0 0 0\n");
        let p = process::parse_pid_stat(&line, PAGE).expect("should parse");
        assert_eq!(p.state, expected, "state {ch}");
    }
}

#[test]
fn keeps_an_unknown_state_rather_than_discarding_the_process() {
    let line = "1 (x) Q 0 0 0 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 0 0 0 0\n";

    let p = process::parse_pid_stat(line, PAGE).expect("should parse");

    assert_eq!(p.state, ProcState::Unknown);
}

#[test]
fn identifies_a_process_by_pid_and_start_time() {
    // Linux recycles PIDs. Keying deltas on the bare pid attributes a dead
    // process's accumulated CPU time to whoever inherits its number.
    let a = process::parse_pid_stat(PID_STAT, PAGE).unwrap();
    let recycled = PID_STAT.replacen(" 9143 ", " 999999 ", 1);
    let b = process::parse_pid_stat(&recycled, PAGE).unwrap();

    assert_eq!(a.pid, b.pid);
    assert_ne!(a.key(), b.key(), "same pid, different start time");
}

#[test]
fn rejects_a_line_that_is_not_a_stat_record() {
    assert!(process::parse_pid_stat("", PAGE).is_none());
    assert!(process::parse_pid_stat("not a stat line\n", PAGE).is_none());
    assert!(process::parse_pid_stat("42 (truncated) S 1\n", PAGE).is_none());
}

#[test]
fn cpu_time_saturates_rather_than_overflowing() {
    let line = format!(
        "1 (x) R 0 0 0 0 -1 0 0 0 0 0 {} {} 0 0 20 0 1 0 0 0 0 0\n",
        u64::MAX,
        u64::MAX
    );

    let p = process::parse_pid_stat(&line, PAGE).expect("should parse");

    assert_eq!(p.cpu_time(), u64::MAX, "saturated, not panicked");
}
