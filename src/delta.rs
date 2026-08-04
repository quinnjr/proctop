//! Rate derivation: turns two consecutive snapshots into values expressed as
//! fractions and per-second rates, so the UI never does accounting
//! arithmetic on raw kernel counters.

use crate::model::CpuTimes;

/// How one CPU spent the interval between two samples, as fractions of that
/// interval. The fields sum to 1.0 across a non-empty interval.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuUsage {
    pub user: f32,
    pub nice: f32,
    pub system: f32,
    pub idle: f32,
    pub iowait: f32,
    pub irq: f32,
    pub softirq: f32,
    pub steal: f32,
    pub guest: f32,
    pub guest_nice: f32,
}

impl CpuUsage {
    /// The fraction of the interval the CPU spent doing work.
    ///
    /// Idle time is obviously excluded; `iowait` is excluded too, because a
    /// CPU blocked on disk is not executing anything. htop draws iowait as
    /// its own meter segment for the same reason.
    ///
    /// This sums the working states rather than computing `1 - idle`, so an
    /// interval with no data reads as 0% busy instead of 100%.
    pub fn busy(&self) -> f32 {
        (self.user
            + self.nice
            + self.system
            + self.irq
            + self.softirq
            + self.steal
            + self.guest
            + self.guest_nice)
            .clamp(0.0, 1.0)
    }
}

/// Bytes-per-second for each device present in `cur`.
///
/// A device with no previous reading reports zero rather than its lifetime
/// total — drives get plugged in and interfaces come up mid-session, and
/// their first frame is not a burst of throughput. Counters that went
/// backwards (an interface brought down and back up) report zero for the
/// same reason they do elsewhere, and a zero-length interval reports zero
/// rather than dividing by it.
pub fn io_rates<T: crate::model::Counters>(
    prev: &[T],
    cur: &[T],
    elapsed: std::time::Duration,
) -> Vec<crate::model::IoRate> {
    let seconds = elapsed.as_secs_f64();
    let previous: std::collections::HashMap<&str, (u64, u64)> = prev
        .iter()
        .map(|d| (d.name(), (d.read_bytes(), d.write_bytes())))
        .collect();

    cur.iter()
        .map(|device| {
            let (read, write) = match previous.get(device.name()) {
                Some(&(read, write)) if seconds > 0.0 => (
                    per_second(device.read_bytes().saturating_sub(read), seconds),
                    per_second(device.write_bytes().saturating_sub(write), seconds),
                ),
                _ => (0, 0),
            };
            crate::model::IoRate {
                name: device.name().to_string(),
                read_per_sec: read,
                write_per_sec: write,
            }
        })
        .collect()
}

fn per_second(bytes: u64, seconds: f64) -> u64 {
    (bytes as f64 / seconds) as u64
}

/// Whether a subsystem read at `last` is due again by `now`.
///
/// Used to pace readings that are far more expensive than the rest of a
/// sample, or that change far more slowly. A zero interval means every time.
pub fn should_refresh(
    last: Option<std::time::Instant>,
    now: std::time::Instant,
    interval: std::time::Duration,
) -> bool {
    match last {
        None => true,
        Some(last) => now.duration_since(last) >= interval,
    }
}

/// Total jiffies elapsed across all cores between two readings of the
/// aggregate `/proc/stat` line — the denominator for per-process usage.
pub fn total_jiffies(prev: &CpuTimes, cur: &CpuTimes) -> u64 {
    let fields = |t: &CpuTimes| {
        // Guest time is already counted inside user/nice, so it is excluded
        // here for the same reason it is subtracted out in `cpu_usage`.
        //
        // Saturating rather than plain `+`: this runs on the sampling task,
        // which the design requires to be panic-free *by construction*, and
        // a debug-build overflow is a panic.
        [
            t.user, t.nice, t.system, t.idle, t.iowait, t.irq, t.softirq, t.steal,
        ]
        .into_iter()
        .fold(0u64, u64::saturating_add)
    };
    fields(cur).saturating_sub(fields(prev))
}

/// One process's CPU consumption over the interval, as a fraction of a
/// single core: `1.0` is one saturated core, `2.0` is two.
///
/// `total_jiffy_delta` is the change in the aggregate `/proc/stat` line,
/// which already sums across every core — dividing by it and multiplying by
/// the core count is what rescales the figure to a single core, the way htop
/// reports it.
///
/// Returns zero for an empty interval, an unknown core count, or a counter
/// that went backwards, rather than NaN or a wrapped-around delta.
pub fn process_cpu(
    prev_cpu_time: u64,
    cur_cpu_time: u64,
    total_jiffy_delta: u64,
    cores: usize,
) -> f32 {
    if total_jiffy_delta == 0 || cores == 0 {
        return 0.0;
    }
    let used = cur_cpu_time.saturating_sub(prev_cpu_time);
    used as f32 / total_jiffy_delta as f32 * cores as f32
}

/// Fraction of the interval represented by each CPU state.
///
/// Two corrections happen here that a naive subtract-and-divide misses:
///
/// * The kernel folds guest time into `user` (and guest_nice into `nice`)
///   *in addition to* reporting it separately. Both are subtracted out, or
///   virtualization hosts double-count the interval and every percentage
///   comes out too small.
/// * Any field that went backwards — suspend/resume, a core going offline,
///   a counter reset — contributes zero rather than a wrapped-around
///   enormous delta.
///
/// An empty interval (two reads inside the same jiffy) yields all zeros
/// instead of NaN.
pub fn cpu_usage(prev: &CpuTimes, cur: &CpuTimes) -> CpuUsage {
    // Guest time is already counted inside user/nice; remove it before
    // taking the delta so it is counted exactly once.
    let split = |t: &CpuTimes| {
        [
            t.user.saturating_sub(t.guest),
            t.nice.saturating_sub(t.guest_nice),
            t.system,
            t.idle,
            t.iowait,
            t.irq,
            t.softirq,
            t.steal,
            t.guest,
            t.guest_nice,
        ]
    };

    let (prev, cur) = (split(prev), split(cur));
    let deltas: [u64; 10] = std::array::from_fn(|i| cur[i].saturating_sub(prev[i]));

    let total: u64 = deltas.iter().copied().fold(0u64, u64::saturating_add);
    if total == 0 {
        return CpuUsage::default();
    }

    let total = total as f32;
    let f = |i: usize| deltas[i] as f32 / total;
    CpuUsage {
        user: f(0),
        nice: f(1),
        system: f(2),
        idle: f(3),
        iowait: f(4),
        irq: f(5),
        softirq: f(6),
        steal: f(7),
        guest: f(8),
        guest_nice: f(9),
    }
}
