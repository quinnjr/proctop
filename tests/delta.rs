use rtop::delta::cpu_usage;
use rtop::model::CpuTimes;

/// Floats here come from ratios of small integers, so they are exact enough
/// to compare tightly; the epsilon only absorbs f32 rounding.
#[track_caller]
fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-5,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn splits_the_interval_into_fractions_of_total_jiffies() {
    let prev = CpuTimes::default();
    let cur = CpuTimes {
        user: 50,
        idle: 50,
        ..CpuTimes::default()
    };

    let usage = cpu_usage(&prev, &cur);

    assert_close(usage.user, 0.5);
    assert_close(usage.idle, 0.5);
    assert_close(usage.busy(), 0.5);
}

#[test]
fn counts_only_the_jiffies_that_elapsed_between_samples() {
    // A machine that was already up: only the 100-jiffy difference counts,
    // not the accumulated totals.
    let prev = CpuTimes {
        user: 1_000,
        system: 500,
        idle: 8_500,
        ..CpuTimes::default()
    };
    let cur = CpuTimes {
        user: 1_025,
        system: 525,
        idle: 8_550,
        ..CpuTimes::default()
    };

    let usage = cpu_usage(&prev, &cur);

    assert_close(usage.user, 0.25);
    assert_close(usage.system, 0.25);
    assert_close(usage.idle, 0.5);
    assert_close(usage.busy(), 0.5);
}

#[test]
fn excludes_iowait_from_busy_time() {
    // Waiting on disk is not the CPU doing work, and htop reports it as its
    // own segment rather than folding it into utilization.
    let prev = CpuTimes::default();
    let cur = CpuTimes {
        user: 25,
        iowait: 25,
        idle: 50,
        ..CpuTimes::default()
    };

    let usage = cpu_usage(&prev, &cur);

    assert_close(usage.iowait, 0.25);
    assert_close(usage.busy(), 0.25);
}

#[test]
fn does_not_double_count_guest_time_already_included_in_user() {
    // The kernel adds guest time into `user` as well as reporting it
    // separately. Counting both inflates the total and shrinks every
    // percentage — a bug that only shows up on virtualization hosts.
    let prev = CpuTimes::default();
    let cur = CpuTimes {
        user: 100, // all of which is guest time
        guest: 100,
        idle: 100,
        ..CpuTimes::default()
    };

    let usage = cpu_usage(&prev, &cur);

    assert_close(usage.user, 0.0);
    assert_close(usage.guest, 0.5);
    assert_close(usage.idle, 0.5);
    assert_close(usage.busy(), 0.5);
}

#[test]
fn reports_zero_rather_than_nan_when_no_time_elapsed() {
    // Two reads inside the same jiffy give a zero denominator. Dividing
    // anyway produces NaN, which renders as a blank meter forever.
    let same = CpuTimes {
        user: 10,
        idle: 90,
        ..CpuTimes::default()
    };

    let usage = cpu_usage(&same, &same);

    assert_close(usage.user, 0.0);
    assert_close(usage.idle, 0.0);
    assert_close(usage.busy(), 0.0);
}

#[test]
fn clamps_counters_that_went_backwards() {
    // Suspend/resume, a CPU going offline and back, or a counter reset can
    // all produce a smaller reading than the one before it.
    let prev = CpuTimes {
        user: 1_000,
        idle: 1_000,
        ..CpuTimes::default()
    };
    let cur = CpuTimes {
        user: 10,
        idle: 1_090,
        ..CpuTimes::default()
    };

    let usage = cpu_usage(&prev, &cur);

    // The user delta is discarded, leaving only the 90 idle jiffies.
    assert_close(usage.user, 0.0);
    assert_close(usage.idle, 1.0);
    assert_close(usage.busy(), 0.0);
}

#[test]
fn keeps_every_fraction_within_zero_and_one() {
    let prev = CpuTimes::default();
    let cur = CpuTimes {
        user: 10,
        nice: 20,
        system: 30,
        idle: 40,
        iowait: 5,
        irq: 3,
        softirq: 2,
        steal: 1,
        ..CpuTimes::default()
    };

    let usage = cpu_usage(&prev, &cur);

    for (name, v) in [
        ("user", usage.user),
        ("nice", usage.nice),
        ("system", usage.system),
        ("idle", usage.idle),
        ("iowait", usage.iowait),
        ("irq", usage.irq),
        ("softirq", usage.softirq),
        ("steal", usage.steal),
        ("busy", usage.busy()),
    ] {
        assert!((0.0..=1.0).contains(&v), "{name} out of range: {v}");
    }
}
