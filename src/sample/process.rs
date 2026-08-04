//! `/proc/[pid]/stat` — per-process accounting.

use crate::model::{Proc, ProcState};

/// Field offsets within the whitespace-separated tail that follows the
/// `comm` field, which is where the fixed-position columns begin. `stat`
/// field numbers are 1-based and count `pid` and `comm` as 1 and 2, so a
/// documented field N sits at index N - 3 here.
mod field {
    pub const STATE: usize = 0; // stat field 3
    pub const PPID: usize = 1; // 4
    pub const UTIME: usize = 11; // 14
    pub const STIME: usize = 12; // 15
    pub const PRIORITY: usize = 15; // 18
    pub const NICE: usize = 16; // 19
    pub const THREADS: usize = 17; // 20
    pub const STARTTIME: usize = 19; // 22
    pub const VSIZE: usize = 20; // 23
    pub const RSS: usize = 21; // 24

    /// Everything through `rss` must be present for the record to be usable.
    pub const REQUIRED: usize = RSS + 1;
}

/// Parse one `/proc/[pid]/stat` record.
///
/// `page_size` converts the RSS column, which the kernel reports in pages.
///
/// Returns `None` for a record that is truncated or not a stat line at all —
/// which happens routinely when a process exits mid-read.
pub fn parse_pid_stat(text: &str, page_size: u64) -> Option<Proc> {
    let line = text.lines().next()?;

    // `comm` is wrapped in parentheses and may contain anything at all,
    // including spaces and parentheses. The kernel does not escape it, so
    // the only reliable delimiters are the first '(' and the LAST ')'.
    // Splitting on whitespace, or scanning for the first ')', silently
    // shifts every subsequent field.
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close < open {
        return None;
    }

    let pid = line[..open].trim().parse().ok()?;
    let name = line[open + 1..close].to_string();

    let rest: Vec<&str> = line[close + 1..].split_whitespace().collect();
    if rest.len() < field::REQUIRED {
        return None;
    }

    Some(Proc {
        pid,
        ppid: num(&rest, field::PPID),
        name,
        state: parse_state(rest[field::STATE]),
        utime: num(&rest, field::UTIME),
        stime: num(&rest, field::STIME),
        priority: num(&rest, field::PRIORITY),
        nice: num(&rest, field::NICE),
        threads: num(&rest, field::THREADS),
        starttime: num(&rest, field::STARTTIME),
        vsize: num(&rest, field::VSIZE),
        rss: num::<u64>(&rest, field::RSS).saturating_mul(page_size),
    })
}

/// Read one already-bounds-checked column.
///
/// The record is known to be well-formed by this point, so a column that
/// fails to parse means an unfamiliar kernel rather than a torn read: it
/// falls back to a default instead of dropping the whole process.
fn num<T: std::str::FromStr + Default>(rest: &[&str], index: usize) -> T {
    rest[index].parse().unwrap_or_default()
}

fn parse_state(field: &str) -> ProcState {
    match field.chars().next() {
        Some('R') => ProcState::Running,
        Some('S') => ProcState::Sleeping,
        Some('D') => ProcState::UninterruptibleSleep,
        Some('Z') => ProcState::Zombie,
        Some('T') => ProcState::Stopped,
        Some('t') => ProcState::TracingStop,
        Some('I') => ProcState::Idle,
        Some('X' | 'x') => ProcState::Dead,
        _ => ProcState::Unknown,
    }
}
