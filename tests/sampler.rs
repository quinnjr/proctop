//! The one place rtop's tests touch the live system.
//!
//! Every parser is covered by fixtures; this only checks that the reader
//! wiring around them finds the real files and produces a coherent sample.

#![cfg(target_os = "linux")]

use rtop::sampler::Sampler;

#[test]
fn samples_the_running_machine() {
    let mut sampler = Sampler::new();

    let sample = sampler.sample();

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

    let sample = sampler.sample();
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

    let first = sampler.sample();

    assert_eq!(first.cpu.busy(), 0.0);
    assert!(first.procs.iter().all(|r| r.cpu == 0.0));
}

#[test]
fn produces_percentages_within_range_on_the_second_sample() {
    let mut sampler = Sampler::new();

    sampler.sample();
    std::thread::sleep(std::time::Duration::from_millis(120));
    let second = sampler.sample();

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
