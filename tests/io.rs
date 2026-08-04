use proctop::delta::io_rates;
use proctop::model::{DiskStat, IoRate, NetStat};
use proctop::sample::{disk, net};

const DISKSTATS: &str = include_str!("fixtures/diskstats.txt");
const NETDEV: &str = include_str!("fixtures/netdev.txt");

#[track_caller]
fn find<'a>(rates: &'a [IoRate], name: &str) -> &'a IoRate {
    rates
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no rate for {name}"))
}

#[test]
fn reads_per_device_sector_counters() {
    // The operation counts are asserted too: nothing else reads them, so a
    // transposed READS/WRITES offset would be a silent mutation.
    // 259 0 nvme1n1 228929 272244 28176355 58872 369982 27282 25024710 ...
    //                  reads      merged  sectors_read      writes  merged  sectors_written
    let disks = disk::parse_diskstats(DISKSTATS);
    let nvme = disks
        .iter()
        .find(|d| d.name == "nvme1n1")
        .expect("fixture has nvme1n1");

    assert_eq!(nvme.sectors_read, 28_176_355);
    assert_eq!(nvme.sectors_written, 25_024_710);
    assert_eq!(nvme.reads, 228_929);
    assert_eq!(nvme.writes, 369_982);
}

#[test]
fn keeps_partitions_alongside_whole_devices() {
    // htop's I/O view shows both; deciding which is "real" is the reader's
    // job, not the parser's.
    let disks = disk::parse_diskstats(DISKSTATS);
    let names: Vec<&str> = disks.iter().map(|d| d.name.as_str()).collect();

    assert!(names.contains(&"sdb"));
    assert!(names.contains(&"sdb1"));
}

#[test]
fn skips_a_malformed_diskstats_line_without_losing_the_rest() {
    let text = format!("garbage\n{DISKSTATS}");

    assert_eq!(
        disk::parse_diskstats(&text).len(),
        disk::parse_diskstats(DISKSTATS).len()
    );
}

#[test]
fn reads_per_interface_byte_counters() {
    //     lo: 113396770  221765 ... 113396770  221765
    let nets = net::parse_netdev(NETDEV);
    let lo = nets
        .iter()
        .find(|n| n.name == "lo")
        .expect("fixture has lo");

    assert_eq!(lo.rx_bytes, 113_396_770);
    assert_eq!(lo.tx_bytes, 113_396_770);
    // Packet counts too — nothing else reads them, so a transposed offset
    // would never be noticed.
    assert_eq!(lo.rx_packets, 221_765);
    assert_eq!(lo.tx_packets, 221_765);
}

#[test]
fn handles_an_interface_name_that_touches_the_colon() {
    // The kernel pads short names but not long ones, so `wlp0s20f3:` has no
    // space before the colon. Splitting on whitespace loses the name.
    let nets = net::parse_netdev(NETDEV);
    let names: Vec<&str> = nets.iter().map(|n| n.name.as_str()).collect();

    assert!(names.contains(&"wlp0s20f3"), "got {names:?}");
    assert!(names.contains(&"eno1"), "got {names:?}");
}

#[test]
fn skips_the_two_header_lines() {
    let nets = net::parse_netdev(NETDEV);

    assert!(nets.iter().all(|n| !n.name.contains("face")));
    assert!(nets.iter().all(|n| !n.name.contains("Inter")));
}

#[test]
fn converts_sector_counts_into_bytes_per_second() {
    // A sector in /proc/diskstats is 512 bytes by convention, regardless of
    // the device's actual block size.
    let before = vec![DiskStat {
        name: "sda".into(),
        sectors_read: 0,
        sectors_written: 0,
        ..DiskStat::default()
    }];
    let after = vec![DiskStat {
        name: "sda".into(),
        sectors_read: 2_000,
        sectors_written: 1_000,
        ..DiskStat::default()
    }];

    let rates = io_rates(&before, &after, std::time::Duration::from_secs(2));

    let sda = find(&rates, "sda");
    assert_eq!(sda.read_per_sec, 2_000 * 512 / 2);
    assert_eq!(sda.write_per_sec, 1_000 * 512 / 2);
}

#[test]
fn reports_zero_for_a_device_that_appeared_since_the_last_sample() {
    // USB drives get plugged in.
    let after = vec![DiskStat {
        name: "sdz".into(),
        sectors_read: 5_000,
        ..DiskStat::default()
    }];

    let rates = io_rates(&[], &after, std::time::Duration::from_secs(1));

    assert_eq!(find(&rates, "sdz").read_per_sec, 0);
}

#[test]
fn reports_zero_rather_than_dividing_by_a_zero_interval() {
    let stats = vec![DiskStat {
        name: "sda".into(),
        sectors_read: 100,
        ..DiskStat::default()
    }];

    let rates = io_rates(&stats, &stats, std::time::Duration::ZERO);

    assert_eq!(find(&rates, "sda").read_per_sec, 0);
}

#[test]
fn computes_network_rates_the_same_way() {
    let before = vec![NetStat {
        name: "eno1".into(),
        rx_bytes: 1_000,
        tx_bytes: 500,
        ..NetStat::default()
    }];
    let after = vec![NetStat {
        name: "eno1".into(),
        rx_bytes: 3_000,
        tx_bytes: 1_500,
        ..NetStat::default()
    }];

    let rates = io_rates(&before, &after, std::time::Duration::from_secs(2));

    let eno1 = find(&rates, "eno1");
    assert_eq!(eno1.read_per_sec, 1_000);
    assert_eq!(eno1.write_per_sec, 500);
}

#[test]
fn clamps_a_counter_that_went_backwards() {
    // Interface counters reset when a device is brought down and back up.
    let before = vec![NetStat {
        name: "eno1".into(),
        rx_bytes: 10_000,
        ..NetStat::default()
    }];
    let after = vec![NetStat {
        name: "eno1".into(),
        rx_bytes: 5,
        ..NetStat::default()
    }];

    let rates = io_rates(&before, &after, std::time::Duration::from_secs(1));

    assert_eq!(find(&rates, "eno1").read_per_sec, 0);
}
