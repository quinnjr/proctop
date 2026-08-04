use rtop::delta::process_cpu;

#[track_caller]
fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-5,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn reports_a_saturated_core_as_one_hundred_percent() {
    // On a 4-core box, 100 of the 400 elapsed jiffies is exactly one core.
    // htop scales to a single core, so this reads as 1.0 (100%), not 0.25.
    assert_close(process_cpu(0, 100, 400, 4), 1.0);
}

#[test]
fn reports_a_process_spanning_multiple_cores_above_one_hundred_percent() {
    // A threaded process can consume more than one core's worth.
    assert_close(process_cpu(0, 200, 400, 4), 2.0);
}

#[test]
fn counts_only_the_jiffies_consumed_since_the_previous_sample() {
    // Cumulative counters: a long-lived idle process must read as 0, not as
    // its lifetime average.
    assert_close(process_cpu(50_000, 50_000, 400, 4), 0.0);
}

#[test]
fn reports_zero_rather_than_nan_when_no_time_elapsed() {
    assert_close(process_cpu(0, 0, 0, 4), 0.0);
}

#[test]
fn reports_zero_when_the_core_count_is_unknown() {
    assert_close(process_cpu(0, 100, 400, 0), 0.0);
}

#[test]
fn clamps_a_counter_that_went_backwards() {
    // Happens when a pid is recycled between samples and the identity check
    // did not catch it.
    assert_close(process_cpu(100, 10, 400, 4), 0.0);
}
