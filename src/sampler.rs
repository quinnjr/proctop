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
use crate::model::{
    CpuStat, DiskStat, IoRate, NetStat, ProcKey, ProcRow, ProcState, Sample, Sensor,
};
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
    /// Previous counters per stream, each with the instant it was read.
    ///
    /// Per-stream rather than one shared timestamp: a stream whose file
    /// could not be read must not advance its own clock, or the next
    /// successful tick divides two intervals of counters by one interval
    /// and reports roughly double the true throughput.
    prev_disks: Option<(Vec<DiskStat>, Instant)>,
    prev_nets: Option<(Vec<NetStat>, Instant)>,
    /// Last hardware readings, kept so switching to the sensors tab shows
    /// something immediately rather than a blank screen.
    sensors: Shared<Vec<Sensor>>,
    sensors_at: Option<Instant>,
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
            prev_disks: None,
            prev_nets: None,
            sensors: Shared::default(),
            sensors_at: None,
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
    ///
    /// Which subsystems report unreadability as such is a deliberate line.
    /// `disks`, `nets`, and `sensors` are `Option`, because a restricted
    /// container or a machine with no `hwmon` can legitimately deny them and
    /// "idle" would be a lie. `mem`, `load`, and `uptime` are not: procfs
    /// guarantees those files on every Linux, so a failure there is not a
    /// case worth a second rendering path — it is a broken kernel, and the
    /// zeros are as good a signal as anything.
    pub fn sample(&mut self, want_sensors: bool) -> Sample {
        // An unreadable `/proc/stat` must not become a baseline: storing a
        // zeroed reading makes the next tick diff a real counter against
        // zero, which reports the machine's since-boot average as if it
        // were this interval, and drops every per-core meter for a frame.
        let cpu_stat = match cpu::parse_stat(&read("/proc/stat")) {
            Some(stat) => stat,
            None => self.prev_cpu.clone().unwrap_or_default(),
        };

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
        let mut threads: i64 = 0;

        for pid in pids() {
            let Some(proc) =
                process::parse_pid_stat(&read(&format!("/proc/{pid}/stat")), self.page_size)
            else {
                // The process exited mid-read. Routine, not an error.
                continue;
            };

            let uid = euid_of(pid);
            let key = proc.key();
            let cpu_time = proc.cpu_time();

            if proc.state == ProcState::Running {
                running += 1;
            }
            // Saturating like every other sum on this path: the column is
            // unvalidated /proc text, and a debug-build overflow is a panic.
            threads = threads.saturating_add(proc.threads);
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
        let disk_rates = rates_from(&mut self.prev_disks, disks, now);
        let net_rates = rates_from(&mut self.prev_nets, nets, now);

        self.prev_cpu = Some(cpu_stat);
        self.prev_times = times;

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
    ///
    /// `/etc/passwd` is read once at startup and these names live for the
    /// session, so a user created later shows as a bare number. The cache is
    /// keyed by uid, not pid, so it is bounded by the distinct uids that own
    /// a process during the run — single digits normally, and a few thousand
    /// at worst under systemd `DynamicUser=`.
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

/// Turn one stream's new counters into rates, and make them the baseline.
///
/// `None` in means the file could not be read: no rates come out and the
/// baseline is left untouched, so the next successful read measures across
/// the whole gap rather than attributing it to one interval.
fn rates_from<T: crate::model::Counters>(
    previous: &mut Option<(Vec<T>, Instant)>,
    current: Option<Vec<T>>,
    now: Instant,
) -> Option<Vec<IoRate>> {
    let current = current?;
    let since = previous
        .as_ref()
        .map(|(_, at)| now.duration_since(*at))
        .unwrap_or(Duration::ZERO);

    let rates = match previous {
        // No baseline yet: `io_rates` reports zero rather than a lifetime
        // total, which is what an empty slice gives it.
        None => io_rates(&[], &current, since),
        Some((stats, _)) => io_rates(stats, &current, since),
    };

    *previous = Some((current, now));
    Some(rates)
}

/// Read a file, distinguishing "could not read it" from "it was empty".
fn read_optional(path: &str) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    // Validated in place on the overwhelmingly common path; the lossy
    // conversion — which allocates a second buffer and copies — is paid for
    // only by the rare record that actually needs it.
    Some(
        String::from_utf8(bytes)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
    )
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
    read_optional(path).unwrap_or_default()
}

/// The uid owning `/proc/<pid>`, which is the process's **effective** uid.
///
/// Not the real uid: the kernel builds this inode's ownership from
/// `cred->euid` (`task_dump_owner()`), and kernel threads are forced to
/// root. For the overwhelming majority of processes the two agree; they
/// diverge for anything running through a setuid binary, which this
/// reports as its target user rather than its invoker.
///
/// That is what htop shows, and it is worth the difference: reading the
/// `Uid:` line instead means opening `/proc/<pid>/status` for every
/// process, and that file is ~55 lines including the VmPeak/VmHWM group,
/// which makes the kernel take the process's `mmap_lock`. It was the most
/// expensive read in the sampling loop, for one integer — removing it
/// roughly halved the cost of a sample.
fn euid_of(pid: i32) -> Option<u32> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DiskStat;

    fn disk(name: &str, sectors: u64) -> DiskStat {
        DiskStat {
            name: name.into(),
            sectors_read: sectors,
            ..DiskStat::default()
        }
    }

    fn rate_of(rates: &Option<Vec<crate::model::IoRate>>, name: &str) -> u64 {
        rates
            .as_ref()
            .expect("rates")
            .iter()
            .find(|r| r.name == name)
            .expect("device")
            .read_per_sec
    }

    #[test]
    fn rates_are_per_second_over_the_interval() {
        let mut prev = None;
        let t0 = Instant::now();

        rates_from(&mut prev, Some(vec![disk("sda", 0)]), t0);
        let rates = rates_from(&mut prev, Some(vec![disk("sda", 2_000)]), t0 + SECOND);

        // 2000 sectors x 512 bytes over one second.
        assert_eq!(rate_of(&rates, "sda"), 2_000 * 512);
    }

    #[test]
    fn a_failed_read_reports_nothing_and_does_not_disturb_the_baseline() {
        let mut prev = None;
        let t0 = Instant::now();
        rates_from(&mut prev, Some(vec![disk("sda", 0)]), t0);

        let during = rates_from(&mut prev, None, t0 + SECOND);

        assert!(during.is_none(), "unreadable is not the same as idle");
    }

    #[test]
    fn the_tick_after_a_failed_read_does_not_report_a_spike() {
        // The regression this pins: the sample clock advanced on the failed
        // tick while the counter baseline did not, so the next frame divided
        // two intervals' worth of counters by one interval and reported ~2x
        // the true throughput.
        let mut prev = None;
        let t0 = Instant::now();
        rates_from(&mut prev, Some(vec![disk("sda", 0)]), t0);
        rates_from(&mut prev, None, t0 + SECOND);

        let after = rates_from(&mut prev, Some(vec![disk("sda", 2_000)]), t0 + SECOND * 2);

        // Two seconds elapsed since the last successful read, so the rate is
        // half what a one-second window would have reported.
        assert_eq!(rate_of(&after, "sda"), 2_000 * 512 / 2);
    }

    #[test]
    fn the_first_successful_read_reports_zero_rather_than_a_lifetime_total() {
        let mut prev = None;

        let first = rates_from(&mut prev, Some(vec![disk("sda", 9_999)]), Instant::now());

        assert_eq!(rate_of(&first, "sda"), 0);
    }

    const SECOND: Duration = Duration::from_secs(1);
}
