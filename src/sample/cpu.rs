//! `/proc/stat` — cumulative per-core and aggregate CPU time.

use crate::model::{CpuStat, CpuTimes};

/// Parse the contents of `/proc/stat`.
///
/// Returns `None` only if the aggregate `cpu` line is absent, which would
/// mean the file is not `/proc/stat` at all. Individual malformed fields are
/// treated as zero rather than failing the whole read — a kernel that grows
/// a new column should not take the monitor down.
pub fn parse_stat(text: &str) -> Option<CpuStat> {
    let mut total = None;
    let mut cores = Vec::new();

    for line in text.lines() {
        let Some(rest) = line.strip_prefix("cpu") else {
            // `/proc/stat` lists the cpu lines first; everything after them
            // (intr, ctxt, btime, ...) is not ours.
            continue;
        };
        // The aggregate line is `cpu` followed by spaces; per-core lines are
        // `cpu0`, `cpu1`, ... Distinguish on the first byte after the prefix.
        match rest.split_once(char::is_whitespace) {
            Some(("", fields)) => total = Some(parse_times(fields)),
            Some((index, fields)) if index.bytes().all(|b| b.is_ascii_digit()) => {
                cores.push(parse_times(fields))
            }
            _ => continue,
        }
    }

    Some(CpuStat {
        total: total?,
        cores,
    })
}

/// Parse the whitespace-separated jiffy columns of one `cpu` line.
///
/// Columns the kernel does not emit (older kernels stop before `steal`) stay
/// at zero, and any column that fails to parse is treated as zero.
fn parse_times(fields: &str) -> CpuTimes {
    let mut it = fields.split_whitespace().map(|f| f.parse().unwrap_or(0));
    let mut next = || it.next().unwrap_or(0);
    CpuTimes {
        user: next(),
        nice: next(),
        system: next(),
        idle: next(),
        iowait: next(),
        irq: next(),
        softirq: next(),
        steal: next(),
        guest: next(),
        guest_nice: next(),
    }
}
