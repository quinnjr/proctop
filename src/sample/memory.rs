//! `/proc/meminfo` — physical memory and swap.

use crate::model::MemInfo;

/// Parse the contents of `/proc/meminfo`.
///
/// Returns `None` only if `MemTotal` is absent, which means the file is not
/// `/proc/meminfo`. Every other field is optional: swap fields are missing
/// on a machine without swap, and `MemAvailable` predates only very old
/// kernels. Absent fields read as zero.
pub fn parse_meminfo(text: &str) -> Option<MemInfo> {
    let mut mem = MemInfo::default();
    let mut total_seen = false;
    // Held separately because `cached` is a computed figure, not a field.
    let (mut page_cache, mut reclaimable, mut shmem) = (0, 0, 0);

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let Some(bytes) = parse_kb(value) else {
            continue;
        };

        match key {
            "MemTotal" => {
                mem.total = bytes;
                total_seen = true;
            }
            "MemFree" => mem.free = bytes,
            "MemAvailable" => mem.available = bytes,
            "Buffers" => mem.buffers = bytes,
            "Cached" => page_cache = bytes,
            "SReclaimable" => reclaimable = bytes,
            "Shmem" => shmem = bytes,
            "SwapTotal" => mem.swap_total = bytes,
            "SwapFree" => mem.swap_free = bytes,
            _ => {}
        }
    }

    if !total_seen {
        return None;
    }

    // htop's definition: page cache plus reclaimable slab, minus shared
    // memory. Shmem lives in the page cache but cannot be reclaimed, so
    // leaving it in would understate used memory on a tmpfs-heavy system.
    mem.cached = (page_cache + reclaimable).saturating_sub(shmem);

    Some(mem)
}

/// Parse a `/proc/meminfo` value — a count followed by an optional unit —
/// into bytes. Every unit the kernel emits here is `kB`.
fn parse_kb(value: &str) -> Option<u64> {
    let mut fields = value.split_whitespace();
    let count: u64 = fields.next()?.parse().ok()?;
    match fields.next() {
        Some("kB") | None => Some(count * 1024),
        // An unrecognized unit means the format changed under us; treating
        // the number as bytes would be a silent 1024x error.
        Some(_) => None,
    }
}
