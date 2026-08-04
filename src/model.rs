//! Types shared across sampling, delta computation, and the UI.

use std::sync::Arc;

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
    ///
    /// Saturating for the same reason every other sum on this path is: the
    /// sampler must be panic-free by construction, and a debug-build
    /// overflow is a panic.
    pub fn cpu_time(&self) -> u64 {
        self.utime.saturating_add(self.stime)
    }

    /// This process's identity across samples.
    pub fn key(&self) -> ProcKey {
        ProcKey {
            pid: self.pid,
            starttime: self.starttime,
        }
    }
}

/// What a [`Sensor`]'s value measures, and therefore how to render it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SensorKind {
    /// Degrees Celsius.
    #[default]
    Temperature,
    /// Revolutions per minute.
    Fan,
    /// Percent charge remaining.
    Battery,
}

/// One hardware reading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sensor {
    /// The hwmon chip or power-supply device it came from. `Core 0` is
    /// meaningless without knowing whether `coretemp` or a motherboard chip
    /// inventing its own names produced it.
    pub chip: String,
    pub label: String,
    pub kind: SensorKind,
    pub value: f32,
}

/// One device's cumulative counters from `/proc/diskstats`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskStat {
    pub name: String,
    pub reads: u64,
    pub writes: u64,
    /// Sectors, which `/proc/diskstats` fixes at 512 bytes regardless of the
    /// device's real block size.
    pub sectors_read: u64,
    pub sectors_written: u64,
}

/// One interface's cumulative counters from `/proc/net/dev`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetStat {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

/// Throughput for one disk or interface, in bytes per second.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IoRate {
    pub name: String,
    pub read_per_sec: u64,
    pub write_per_sec: u64,
}

/// A source of two cumulative byte-ish counters that can be turned into
/// rates. Implemented for both disks and interfaces so the delta logic —
/// including the wraparound and zero-interval guards — exists once.
pub trait Counters {
    fn name(&self) -> &str;
    /// Cumulative bytes in the "read" direction.
    fn read_bytes(&self) -> u64;
    /// Cumulative bytes in the "write" direction.
    fn write_bytes(&self) -> u64;
}

/// `/proc/diskstats` counts sectors, and fixes a sector at 512 bytes here
/// regardless of the device's actual block size.
pub const SECTOR_BYTES: u64 = 512;

impl Counters for DiskStat {
    fn name(&self) -> &str {
        &self.name
    }
    fn read_bytes(&self) -> u64 {
        self.sectors_read.saturating_mul(SECTOR_BYTES)
    }
    fn write_bytes(&self) -> u64 {
        self.sectors_written.saturating_mul(SECTOR_BYTES)
    }
}

impl Counters for NetStat {
    fn name(&self) -> &str {
        &self.name
    }
    fn read_bytes(&self) -> u64 {
        self.rx_bytes
    }
    fn write_bytes(&self) -> u64 {
        self.tx_bytes
    }
}

/// Which `/proc/net` file a socket came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Protocol {
    #[default]
    Tcp,
    Tcp6,
    Udp,
    Udp6,
}

impl Protocol {
    /// The label shown in the PROTO column.
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Tcp => "tcp",
            Protocol::Tcp6 => "tcp6",
            Protocol::Udp => "udp",
            Protocol::Udp6 => "udp6",
        }
    }

    /// Whether addresses in this file are 16 bytes rather than 4.
    pub fn is_v6(self) -> bool {
        matches!(self, Protocol::Tcp6 | Protocol::Udp6)
    }

    /// UDP has no listening state and no accept queue.
    pub fn is_tcp(self) -> bool {
        matches!(self, Protocol::Tcp | Protocol::Tcp6)
    }
}

/// How reachable a bound address is — the fact this view exists to show.
///
/// Ordered so sorting puts the attack surface first: a wildcard bind is
/// reachable from any interface, a loopback bind from none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Exposure {
    /// Bound to every interface — `0.0.0.0` or `::`.
    Exposed,
    /// Bound to one specific address.
    Interface,
    /// Reachable only from this machine.
    Loopback,
}

impl Exposure {
    pub fn of(addr: &std::net::SocketAddr) -> Exposure {
        let ip = addr.ip();
        if ip.is_unspecified() {
            Exposure::Exposed
        } else if ip.is_loopback() {
            Exposure::Loopback
        } else {
            Exposure::Interface
        }
    }

    /// The word shown beside the address.
    pub fn label(self) -> &'static str {
        match self {
            Exposure::Exposed => "all",
            Exposure::Interface => "iface",
            Exposure::Loopback => "local",
        }
    }
}

/// One socket the machine is listening on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Socket {
    pub protocol: Protocol,
    pub local: std::net::SocketAddr,
    pub uid: u32,
    /// Socket inode — the join key to a process via `/proc/<pid>/fd`.
    pub inode: u64,
    /// Connections established and waiting to be accepted. `None` for UDP,
    /// which has no accept queue at all — a different fact from having an
    /// empty one.
    pub accept_queue: Option<u32>,
}

impl Socket {
    pub fn exposure(&self) -> Exposure {
        Exposure::of(&self.local)
    }
}

/// A listening socket with its owner resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListeningSocket {
    pub socket: Socket,
    pub user: Arc<str>,
    /// `None` when the owning process could not be determined, which is the
    /// normal unprivileged case rather than an error: `/proc/<pid>/fd` is
    /// readable only for your own processes.
    pub process: Option<(i32, Arc<str>)>,
}

/// A process plus the figures derived from comparing it against the previous
/// sample. This is what the table renders.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcRow {
    pub proc: Proc,
    /// CPU consumption as a fraction of one core: `1.0` is a saturated core.
    pub cpu: f32,
    /// Resident memory as a fraction of physical memory.
    pub mem: f32,
    /// The owning username, or the bare uid when this machine has no passwd
    /// entry for it — common under containers and directory services.
    ///
    /// Shared rather than owned: a machine has a handful of distinct users
    /// and hundreds of processes, so cloning a row costs a refcount bump
    /// instead of a string copy.
    pub user: std::sync::Arc<str>,
    /// Nesting depth in the tree view; always 0 in the flat view.
    pub depth: usize,
}

/// One complete reading of the machine, with every rate already derived.
///
/// The UI formats these values and does no accounting arithmetic of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    /// Aggregate CPU usage across all cores.
    pub cpu: crate::delta::CpuUsage,
    /// Per-core usage, in the order the kernel lists them.
    pub cores: Vec<crate::delta::CpuUsage>,
    pub mem: MemInfo,
    pub load: LoadAvg,
    pub uptime: std::time::Duration,
    pub procs: Vec<ProcRow>,
    /// Processes in the runnable state at the moment of the sample.
    pub running: usize,
    /// Threads across all processes.
    pub threads: i64,
    /// Per-device disk throughput, or `None` when `/proc/diskstats` could
    /// not be read at all.
    pub disks: Option<Vec<IoRate>>,
    /// Per-interface network throughput, or `None` when `/proc/net/dev`
    /// could not be read at all.
    pub nets: Option<Vec<IoRate>>,
    /// Hardware readings, or `None` when they were not read for this
    /// sample — they are only read while something is displaying them.
    ///
    /// `None` and `Some(vec![])` are deliberately different throughout this
    /// struct: the first means "not read", the second "this machine
    /// genuinely has none", and the views say something different about
    /// each. Reporting an unreadable subsystem as empty is a lie a
    /// restricted container would be told.
    pub sensors: Option<ntui::Shared<Vec<Sensor>>>,
    /// Sockets the machine is listening on, or `None` when they were not
    /// read for this sample — attribution is expensive, so they are read
    /// only while something is displaying them.
    pub sockets: Option<ntui::Shared<Vec<ListeningSocket>>>,
}

impl Default for ProcRow {
    fn default() -> Self {
        ProcRow {
            proc: Proc::default(),
            cpu: 0.0,
            mem: 0.0,
            user: Arc::from(""),
            depth: 0,
        }
    }
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
