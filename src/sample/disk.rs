//! `/proc/diskstats` — per-device cumulative I/O counters.

use crate::model::DiskStat;

/// Field offsets after the major/minor/name prefix, from the kernel's
/// `Documentation/admin-guide/iostats.rst`. Numbers there are 1-based over
/// the whole line, so field N sits at index N - 4 here.
mod field {
    pub const READS: usize = 0; // 4: reads completed
    pub const SECTORS_READ: usize = 2; // 6
    pub const WRITES: usize = 4; // 8: writes completed
    pub const SECTORS_WRITTEN: usize = 6; // 10
    pub const REQUIRED: usize = SECTORS_WRITTEN + 1;
}

/// Parse the contents of `/proc/diskstats`.
///
/// Partitions are kept alongside whole devices: htop's I/O view shows both,
/// and deciding which is "real" is the reader's job, not the parser's.
/// Malformed lines are skipped rather than failing the file.
pub fn parse_diskstats(text: &str) -> Vec<DiskStat> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<DiskStat> {
    let mut fields = line.split_whitespace();
    let _major: u32 = fields.next()?.parse().ok()?;
    let _minor: u32 = fields.next()?.parse().ok()?;
    let name = fields.next()?.to_string();

    let rest: Vec<&str> = fields.collect();
    if rest.len() < field::REQUIRED {
        return None;
    }
    let num = |i: usize| rest[i].parse().unwrap_or(0);

    Some(DiskStat {
        name,
        reads: num(field::READS),
        writes: num(field::WRITES),
        sectors_read: num(field::SECTORS_READ),
        sectors_written: num(field::SECTORS_WRITTEN),
    })
}
