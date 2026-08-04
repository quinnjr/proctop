//! The one place rtop's tests touch the live system.
//!
//! Every parser is covered by fixtures; this only checks that the reader
//! wiring around them finds the real files and produces a coherent sample.

#![cfg(target_os = "linux")]

use rtop::sampler::{Sampler, Wanted};

#[test]
fn samples_the_running_machine() {
    let mut sampler = Sampler::new();

    let sample = sampler.sample(Wanted::default());

    assert!(sample.mem.total > 0, "the machine has memory");
    assert!(
        !sample.cores.is_empty(),
        "the machine has at least one core"
    );
    assert!(
        sample.procs.len() > 1,
        "at least this test process is running"
    );
}

#[test]
fn finds_the_test_process_itself() {
    let me = std::process::id() as i32;
    let mut sampler = Sampler::new();

    let sample = sampler.sample(Wanted::default());
    let row = sample
        .procs
        .iter()
        .find(|r| r.proc.pid == me)
        .expect("the sampler should see the process doing the sampling");

    assert!(!row.proc.name.is_empty());
    assert!(row.proc.rss > 0, "a running process has resident memory");
}

#[test]
fn reports_no_cpu_usage_on_the_very_first_sample() {
    // There is no previous snapshot to diff against, so every percentage is
    // unknown. Reporting anything other than zero would be invented data —
    // and htop shows a first frame of zeros for the same reason.
    let mut sampler = Sampler::new();

    let first = sampler.sample(Wanted::default());

    assert_eq!(first.cpu.busy(), 0.0);
    assert!(first.procs.iter().all(|r| r.cpu == 0.0));
}

#[test]
fn produces_percentages_within_range_on_the_second_sample() {
    let mut sampler = Sampler::new();

    sampler.sample(Wanted::default());
    std::thread::sleep(std::time::Duration::from_millis(120));
    let second = sampler.sample(Wanted::default());

    assert!((0.0..=1.0).contains(&second.cpu.busy()));
    let cores = second.cores.len() as f32;
    for row in &second.procs {
        assert!(
            (0.0..=cores).contains(&row.cpu),
            "{} reported {} cores' worth of CPU",
            row.proc.name,
            row.cpu
        );
        assert!((0.0..=1.0).contains(&row.mem));
    }
}

// ---------- sensors ----------

#[test]
fn does_not_read_sensors_unless_they_are_being_looked_at() {
    // The 80-odd hwmon files on this machine cost ~30ms to read, several
    // times the whole rest of a sample, because many are real hardware I/O
    // over SMBus. Paying that every tick for a tab nobody has open is what
    // pushed rtop over its CPU budget.
    let mut sampler = Sampler::new();

    let sample = sampler.sample(Wanted::default());

    // `None`, not an empty list: "not read" and "this machine has none" are
    // different facts, and the sensors view says something different about
    // each. Conflating them makes a tab that has not sampled yet claim the
    // hardware does not exist.
    assert_eq!(sample.sensors, None);
}

#[test]
fn reads_sensors_when_they_are() {
    let mut sampler = Sampler::new();

    let sample = sampler.sample(Wanted {
        sensors: true,
        ..Wanted::default()
    });

    let sensors = sample.sensors.expect("asked for, so read");
    // Not every machine has hwmon — VMs and containers generally do not —
    // so this only asserts the readings are coherent if any came back.
    for sensor in sensors.iter() {
        assert!(!sensor.chip.is_empty());
        assert!(!sensor.label.is_empty());
        assert!(sensor.value.is_finite());
    }
}

#[test]
fn keeps_showing_the_last_sensor_reading_between_refreshes() {
    // Switching to the sensors tab must not show a blank screen while the
    // next refresh is due.
    let mut sampler = Sampler::new();
    let want = Wanted {
        sensors: true,
        ..Wanted::default()
    };
    let first = sampler.sample(want);

    let second = sampler.sample(want);

    assert_eq!(first.sensors, second.sensors);
}

// ---------- listening sockets ----------

#[test]
fn does_not_read_sockets_unless_they_are_being_looked_at() {
    // Attributing a socket to a process means walking every /proc/<pid>/fd
    // and readlinking each entry — ~12ms on a normal machine, several times
    // the rest of a sample. Not worth paying for a tab nobody has open.
    let mut sampler = Sampler::new();

    let sample = sampler.sample(Wanted::default());

    assert_eq!(sample.sockets, None);
}

#[test]
fn reads_sockets_when_they_are() {
    let mut sampler = Sampler::new();

    let sample = sampler.sample(Wanted {
        sockets: true,
        ..Wanted::default()
    });

    let sockets = sample.sockets.expect("asked for, so read");
    // Any Linux box is listening on something, but assert only on coherence
    // rather than on a particular service being present.
    for listening in sockets.iter() {
        assert!(!listening.user.is_empty());
        assert!(listening.socket.local.port() > 0);
    }
}

#[test]
fn attributes_our_own_listening_socket_to_this_process() {
    // Hermetic: we bind the listener, so the assertion is about the join
    // rather than about whether the host happens to run any services. The
    // previous form was `ours == 0 || attributed > 0`, which asserted
    // nothing at all on a container with no listeners — and the inode
    // filter it covers could have been built from the wrong field with the
    // suite still green.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("should bind");
    let port = listener
        .local_addr()
        .expect("should have an address")
        .port();

    let mut sampler = Sampler::new();
    let sample = sampler.sample(Wanted {
        sockets: true,
        ..Wanted::default()
    });
    let sockets = sample.sockets.expect("asked for");

    let ours = sockets
        .iter()
        .find(|s| s.socket.local.port() == port)
        .unwrap_or_else(|| panic!("our own listener on {port} should appear"));

    assert_eq!(
        ours.process.as_ref().map(|(pid, _)| *pid),
        Some(std::process::id() as i32),
        "the inode join should reach this process"
    );
}

#[test]
fn keeps_showing_the_last_socket_reading_between_refreshes() {
    let mut sampler = Sampler::new();
    let first = sampler.sample(Wanted {
        sockets: true,
        ..Wanted::default()
    });

    let second = sampler.sample(Wanted {
        sockets: true,
        ..Wanted::default()
    });

    assert_eq!(first.sockets, second.sockets);
}

#[test]
fn the_bench_still_measures_every_gated_subsystem() {
    // `examples/bench.rs` enumerates the four `Wanted` combinations, and it
    // is the only place that does. It measured `Wanted::default()` alone
    // once — the one configuration that excludes both expensive subsystems
    // — so a regression in either was invisible to the tool credited with
    // catching exactly that.
    let bench = include_str!("../examples/bench.rs");

    for combination in [
        "sensors: false,\n            sockets: false,",
        "sensors: true,\n            sockets: false,",
        "sensors: false,\n            sockets: true,",
        "sensors: true,\n            sockets: true,",
    ] {
        assert!(
            bench.contains(combination),
            "the bench should still time this combination:\n{combination}"
        );
    }
}
