use proctop::sample::system;

const LOADAVG: &str = include_str!("fixtures/loadavg.txt");
const UPTIME: &str = include_str!("fixtures/uptime.txt");

#[test]
fn parses_load_average_and_task_counts() {
    // 16.31 13.55 6.73 4/2612 95881
    let load = system::parse_loadavg(LOADAVG).expect("fixture should parse");

    assert!((load.one - 16.31).abs() < 1e-4);
    assert!((load.five - 13.55).abs() < 1e-4);
    assert!((load.fifteen - 6.73).abs() < 1e-4);
    assert_eq!(load.running, 4);
    assert_eq!(load.total, 2612);
}

#[test]
fn parses_uptime_in_whole_seconds() {
    // 360.43 7108.14
    let uptime = system::parse_uptime(UPTIME).expect("fixture should parse");

    assert_eq!(uptime.as_secs(), 360);
}

#[test]
fn rejects_a_loadavg_line_that_is_not_one() {
    assert!(system::parse_loadavg("").is_none());
    assert!(system::parse_loadavg("16.31 13.55\n").is_none());
}
