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
    sensors: Vec<Sensor>,
    sensors_at: Option<Instant>,
    /// Wall-clock of the previous sample. Disk and network rates are
    /// per-second, so unlike CPU they need real elapsed time rather than a
    /// jiffy delta.
    prev_at: Option<Instant>,
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
            sensors: Vec::new(),
            sensors_at: None,
            prev_at: None,
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

        let mut procs = Vec::new();
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

            let uid = process::parse_status_uid(&read(&format!("/proc/{pid}/status")));
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

        let disks = disk::parse_diskstats(&read("/proc/diskstats"));
        let nets = net::parse_netdev(&read("/proc/net/dev"));
        let now = Instant::now();
        let since = self
            .prev_at
            .map(|at| now.duration_since(at))
            .unwrap_or(Duration::ZERO);

        let disk_rates = io_rates(&self.prev_disks, &disks, since);
        let net_rates = io_rates(&self.prev_nets, &nets, since);

        self.prev_cpu = Some(cpu_stat);
        self.prev_times = times;
        self.prev_disks = disks;
        self.prev_nets = nets;
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
    fn sensors(&mut self, want: bool, now: Instant) -> Option<Vec<Sensor>> {
        if !want {
            return None;
        }
        if should_refresh(self.sensors_at, now, SENSOR_INTERVAL) {
            self.sensors = read_sensors();
            self.sensors_at = Some(now);
        }
        Some(self.sensors.clone())
    }

    /// The username for a uid, falling back to the number itself when this
    /// machine has no passwd entry — common under containers and directory
    /// services, where hiding the row would be worse than showing a number.
    fn username(&self, uid: Option<u32>) -> String {
        match uid {
            Some(uid) => self
                .users
                .name(uid)
                .map(str::to_string)
                .unwrap_or_else(|| uid.to_string()),
            None => String::from("?"),
        }
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

/// Read a file, treating any failure as empty content. Every parser rejects
/// empty input, so an unreadable file degrades to a missing subsystem rather
/// than an error path of its own.
fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
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
