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

/// One reading of `/proc/loadavg`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoadAvg {
    pub one: f32,
    pub five: f32,
    pub fifteen: f32,
    /// Tasks currently runnable.
    pub running: u32,
    /// Tasks that exist at all.
    pub total: u32,
}

/// The single-character process state from `/proc/[pid]/stat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcState {
    Running,
    Sleeping,
    /// Blocked in the kernel and not killable — usually waiting on I/O.
    UninterruptibleSleep,
    Zombie,
    Stopped,
    TracingStop,
    Idle,
    Dead,
    /// A state this kernel reports that rtop does not recognize. Kept rather
    /// than discarded, so an unfamiliar kernel does not hide processes.
    #[default]
    Unknown,
}

impl ProcState {
    /// The letter htop displays in the `S` column.
    pub fn as_char(self) -> char {
        match self {
            ProcState::Running => 'R',
            ProcState::Sleeping => 'S',
            ProcState::UninterruptibleSleep => 'D',
            ProcState::Zombie => 'Z',
            ProcState::Stopped => 'T',
            ProcState::TracingStop => 't',
            ProcState::Idle => 'I',
            ProcState::Dead => 'X',
            ProcState::Unknown => '?',
        }
    }
}

/// Identifies a process across samples.
///
/// The start time is part of the key because Linux recycles PIDs: without
/// it, a dead process's accumulated CPU time is attributed to whatever new
/// process inherits its number, producing impossible percentages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcKey {
    pub pid: i32,
    pub starttime: u64,
}

/// One process, as of a single sample. CPU times are cumulative jiffies;
/// memory figures are bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Proc {
    pub pid: i32,
    pub ppid: i32,
    pub name: String,
    pub state: ProcState,
    pub utime: u64,
    pub stime: u64,
    pub priority: i64,
    pub nice: i64,
    pub threads: i64,
    /// Jiffies since boot at which this process started.
    pub starttime: u64,
    pub vsize: u64,
    pub rss: u64,
}

impl Proc {
    /// Total CPU time consumed since the process started.
    pub fn cpu_time(&self) -> u64 {
        self.utime + self.stime
    }

    /// This process's identity across samples.
    pub fn key(&self) -> ProcKey {
        ProcKey {
            pid: self.pid,
            starttime: self.starttime,
        }
    }
}

/// A process plus the figures derived from comparing it against the previous
/// sample. This is what the table renders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcRow {
    pub proc: Proc,
    /// CPU consumption as a fraction of one core: `1.0` is a saturated core.
    pub cpu: f32,
    /// Resident memory as a fraction of physical memory.
    pub mem: f32,
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
