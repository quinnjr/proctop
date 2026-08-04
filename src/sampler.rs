//! The reader layer: walks `/proc` and feeds the parsers.
//!
//! This is the only module that performs I/O. Every failure here is expected
//! rather than exceptional — a process that exits between the directory
//! listing and the read of its `stat` file yields `ENOENT` on every tick of a
//! busy machine — so errors drop the affected item and never propagate.

use std::collections::HashMap;
use std::fs;
use std::time::{Duration, Instant};

use crate::delta::{cpu_usage, io_rates, process_cpu, should_refresh, total_jiffies};
use crate::model::{CpuStat, DiskStat, NetStat, ProcKey, ProcRow, ProcState, Sample, Sensor};
use crate::sample::{cpu, disk, memory, net, process, sensors, system, users};
use ntui::Shared;

/// Samples the live machine, retaining the previous reading so rates can be
/// derived.
#[derive(Debug)]
pub struct Sampler {
    page_size: u64,
    users: users::UserTable,
    prev_cpu: Option<CpuStat>,
    /// Cumulative CPU time per process as of the previous sample.
    prev_times: HashMap<ProcKey, u64>,
    prev_disks: Vec<DiskStat>,
    prev_nets: Vec<NetStat>,
    /// Last hardware readings, kept so switching to the sensors tab shows
    /// something immediately rather than a blank screen.
    sensors: Shared<Vec<Sensor>>,
    sensors_at: Option<Instant>,
    /// Wall-clock of the previous sample. Disk and network rates are
    /// per-second, so unlike CPU they need real elapsed time rather than a
    /// jiffy delta.
    prev_at: Option<Instant>,
    /// Interned usernames, so a row's user costs a refcount bump rather
    /// than an allocation.
    user_names: HashMap<u32, std::sync::Arc<str>>,
    unknown_user: std::sync::Arc<str>,
}

impl Sampler {
    pub fn new() -> Self {
        Sampler {
            page_size: procfs::page_size(),
            users: users::parse_passwd(&read("/etc/passwd")),
            prev_cpu: None,
            prev_times: HashMap::new(),
            prev_disks: Vec::new(),
            prev_nets: Vec::new(),
            sensors: Shared::default(),
            sensors_at: None,
            prev_at: None,
            user_names: HashMap::new(),
            unknown_user: "?".into(),
        }
    }

    /// Take a reading.
    ///
    /// Never fails: a subsystem that cannot be read yields its default
    /// rather than taking the monitor down. The first sample reports zero
    /// for every rate, because there is no previous reading to diff against
    /// and inventing a figure would be worse than showing none.
    pub fn sample(&mut self, want_sensors: bool) -> Sample {
        let cpu_stat = cpu::parse_stat(&read("/proc/stat")).unwrap_or_default();

        let (aggregate, cores, elapsed) = match &self.prev_cpu {
            Some(prev) => (
                cpu_usage(&prev.total, &cpu_stat.total),
                // Zip rather than index: a core going offline between
                // samples shortens the list, and indexing would panic.
                prev.cores
                    .iter()
                    .zip(&cpu_stat.cores)
                    .map(|(p, c)| cpu_usage(p, c))
                    .collect(),
                total_jiffies(&prev.total, &cpu_stat.total),
            ),
            None => (
                Default::default(),
                vec![Default::default(); cpu_stat.cores.len()],
                0,
            ),
        };

        let mem = memory::parse_meminfo(&read("/proc/meminfo")).unwrap_or_default();
        let core_count = cpu_stat.cores.len();

        let mut procs = Vec::with_capacity(self.prev_times.len().max(64));
        let mut times = HashMap::new();
        let mut running = 0;
        let mut threads = 0;

        for pid in pids() {
            let Some(proc) =
                process::parse_pid_stat(&read(&format!("/proc/{pid}/stat")), self.page_size)
            else {
                // The process exited mid-read. Routine, not an error.
                continue;
            };

            // The owner of `/proc/<pid>` is the process's real uid, so one
            // `stat(2)` answers what reading `/proc/<pid>/status` would.
            // That file is ~55 lines and includes the VmPeak/VmHWM group,
            // which makes the kernel take the process's mmap_lock — the most
            // expensive read in this loop, for one integer.
            let uid = uid_of(pid);
            let key = proc.key();
            let cpu_time = proc.cpu_time();

            if proc.state == ProcState::Running {
                running += 1;
            }
            threads += proc.threads;
            times.insert(key, cpu_time);

            let cpu = self
                .prev_times
                .get(&key)
                .map(|&prev| process_cpu(prev, cpu_time, elapsed, core_count))
                .unwrap_or(0.0);

            let mem_fraction = if mem.total == 0 {
                0.0
            } else {
                proc.rss as f32 / mem.total as f32
            };

            procs.push(ProcRow {
                user: self.username(uid),
                proc,
                cpu,
                mem: mem_fraction,
                depth: 0,
            });
        }

        // `None` distinguishes "could not read the file" from "read it and
        // there is nothing there" — the same distinction `sensors` makes,
        // and for the same reason: a restricted container reporting its
        // disks as idle rather than as unavailable is a lie.
        let disks = read_optional("/proc/diskstats").map(|t| disk::parse_diskstats(&t));
        let nets = read_optional("/proc/net/dev").map(|t| net::parse_netdev(&t));
        let now = Instant::now();
        let since = self
            .prev_at
            .map(|at| now.duration_since(at))
            .unwrap_or(Duration::ZERO);

        let disk_rates = disks.as_ref().map(|d| io_rates(&self.prev_disks, d, since));
        let net_rates = nets.as_ref().map(|n| io_rates(&self.prev_nets, n, since));

        self.prev_cpu = Some(cpu_stat);
        self.prev_times = times;
        if let Some(disks) = disks {
            self.prev_disks = disks;
        }
        if let Some(nets) = nets {
            self.prev_nets = nets;
        }
        self.prev_at = Some(now);

        Sample {
            cpu: aggregate,
            cores,
            mem,
            load: system::parse_loadavg(&read("/proc/loadavg")).unwrap_or_default(),
            uptime: system::parse_uptime(&read("/proc/uptime")).unwrap_or_default(),
            procs,
            running,
            threads,
            disks: disk_rates,
            nets: net_rates,
            sensors: self.sensors(want_sensors, now),
        }
    }

    /// Hardware readings, refreshed at most every [`SENSOR_INTERVAL`] and
    /// only while something is actually displaying them.
    ///
    /// This machine has 82 hwmon inputs and reading them costs roughly 30ms
    /// — several times the whole rest of a sample — because many are real
    /// I/O over SMBus rather than cached kernel values. Paying that every
    /// tick for a tab nobody has open put rtop well over its CPU budget,
    /// and temperatures do not change fast enough to justify it even when
    /// the tab is open.
    fn sensors(&mut self, want: bool, now: Instant) -> Option<Shared<Vec<Sensor>>> {
        if !want {
            return None;
        }
        if should_refresh(self.sensors_at, now, SENSOR_INTERVAL) {
            self.sensors = Shared::new(read_sensors());
            self.sensors_at = Some(now);
        }
        // A pointer clone: the whole point of the interval is that the ticks
        // between refreshes are free, and deep-copying 82 readings with two
        // owned strings each is not free.
        Some(self.sensors.clone())
    }

    /// The username for a uid, falling back to the number itself when this
    /// machine has no passwd entry — common under containers and directory
    /// services, where hiding the row would be worse than showing a number.
    fn username(&mut self, uid: Option<u32>) -> std::sync::Arc<str> {
        let Some(uid) = uid else {
            return self.unknown_user.clone();
        };
        // Cached because a machine has a handful of distinct users and
        // hundreds of processes; without this every row allocates its own
        // copy of "root" on every tick.
        if let Some(name) = self.user_names.get(&uid) {
            return name.clone();
        }
        let name: std::sync::Arc<str> = match self.users.name(uid) {
            Some(name) => name.into(),
            None => uid.to_string().into(),
        };
        self.user_names.insert(uid, name.clone());
        name
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

/// How often hardware readings are refreshed while they are on screen.
/// Temperatures and fan speeds move on a scale of seconds.
const SENSOR_INTERVAL: Duration = Duration::from_secs(2);

/// Collect every hwmon reading and battery on the machine.
///
/// A box with no `hwmon` at all, or one where the files are unreadable,
/// yields an empty list — the sensors view says so rather than the app
/// treating it as an error.
fn read_sensors() -> Vec<Sensor> {
    let mut out = Vec::new();

    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for chip in entries.flatten() {
            let path = chip.path();
            let name = fs::read_to_string(path.join("name"))
                .map(|n| n.trim().to_string())
                .unwrap_or_else(|_| chip.file_name().to_string_lossy().into_owned());

            let Ok(files) = fs::read_dir(&path) else {
                continue;
            };
            // Only `*_input` files and their labels matter, so the rest are
            // never read — a chip directory holds dozens of files.
            let contents: Vec<(String, String)> = files
                .flatten()
                .filter_map(|f| {
                    let file_name = f.file_name().to_string_lossy().into_owned();
                    if !file_name.ends_with("_input") && !file_name.ends_with("_label") {
                        return None;
                    }
                    Some((file_name, fs::read_to_string(f.path()).ok()?))
                })
                .collect();

            out.extend(sensors::parse_hwmon(&name, &contents));
        }
    }

    if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
        for supply in entries.flatten() {
            let path = supply.path();
            // Only batteries have a capacity; mains adapters do not.
            let (Ok(capacity), Ok(status)) = (
                fs::read_to_string(path.join("capacity")),
                fs::read_to_string(path.join("status")),
            ) else {
                continue;
            };
            let name = supply.file_name().to_string_lossy().into_owned();
            out.extend(sensors::parse_battery(&name, &capacity, &status));
        }
    }

    out
}

/// Read a file, distinguishing "could not read it" from "it was empty".
fn read_optional(path: &str) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Read a file, treating any failure as empty content.
///
/// Parsers that can fail reject empty input; the rest yield an empty
/// collection. Either way an unreadable file degrades to a missing
/// subsystem rather than an error path of its own.
///
/// Decoded lossily rather than with `read_to_string`, which fails outright
/// on invalid UTF-8. A process's `comm` is raw kernel bytes — whatever
/// landed in `argv[0]` or `prctl(PR_SET_NAME)` — with no encoding
/// guarantee, and rejecting the whole record would drop a live, healthy
/// process from every tick rather than the transient exit race this
/// function is otherwise about.
fn read(path: &str) -> String {
    fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

/// The real uid owning `/proc/<pid>`, which is the process's own.
fn uid_of(pid: i32) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(format!("/proc/{pid}")).ok().map(|m| m.uid())
}

/// The pids currently present in `/proc`.
///
/// Some of these will have exited by the time their `stat` file is read;
/// that is expected and handled at the call site.
fn pids() -> Vec<i32> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse().ok())
        .collect()
}
