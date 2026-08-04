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
    let deltas: [u64; 10] =
        std::array::from_fn(|i| cur[i].saturating_sub(prev[i]));

    let total: u64 = deltas.iter().sum();
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
