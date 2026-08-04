//! Types shared across sampling, delta computation, and the UI.

/// Cumulative CPU time in USER_HZ jiffies, as reported by a `cpu` line in
/// `/proc/stat`. Every field is monotonic while the machine is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
    pub guest: u64,
    pub guest_nice: u64,
}

/// One reading of `/proc/stat`: the aggregate `cpu` line plus one entry per
/// core, in the order the kernel lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CpuStat {
    pub total: CpuTimes,
    pub cores: Vec<CpuTimes>,
}

/// One reading of `/proc/meminfo`, in bytes.
///
/// The kernel reports kilobytes; conversion happens in the parser so that no
/// other code has to remember the unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub total: u64,
    pub free: u64,
    /// The kernel's own estimate of what a new allocation could obtain,
    /// which is not simply `free + cached`.
    pub available: u64,
    pub buffers: u64,
    /// Page cache plus reclaimable slab, less shared memory — the segment
    /// htop draws separately because it is reclaimable under pressure.
    pub cached: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

impl MemInfo {
    /// Memory that is genuinely spoken for: everything that is neither free
    /// nor reclaimable.
    pub fn used(&self) -> u64 {
        self.total
            .saturating_sub(self.free)
            .saturating_sub(self.buffers)
            .saturating_sub(self.cached)
    }

    /// Swap in use. Zero on a machine without swap.
    pub fn swap_used(&self) -> u64 {
        self.swap_total.saturating_sub(self.swap_free)
    }
}
