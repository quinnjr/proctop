//! Comparators over the process list, one per sortable column.

use crate::model::ProcRow;

/// A sortable column of the process table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    Pid,
    Name,
    #[default]
    Cpu,
    Memory,
    /// Accumulated CPU time since the process started.
    Time,
}

/// Order the table by `key`.
///
/// Ties always fall back to ascending PID, in both directions. Most
/// processes sit at 0% CPU, so without a deterministic tie-break the idle
/// bulk of the table reshuffles on every refresh and becomes unreadable.
pub fn sort_procs(rows: &mut [ProcRow], key: SortKey, descending: bool) {
    rows.sort_by(|a, b| {
        let ordering = match key {
            SortKey::Pid => a.proc.pid.cmp(&b.proc.pid),
            // Case-insensitive, or every capitalized process sorts ahead of
            // every lowercase one and the column reads as random.
            SortKey::Name => compare_names(&a.proc.name, &b.proc.name),
            // NaN is treated as 0.0 rather than compared: `partial_cmp`
            // reports the pair as incomparable (scrambling the list), and
            // `total_cmp` sorts NaN above infinity (floating it to the top).
            // An unknown percentage renders as zero, so it sorts as zero.
            SortKey::Cpu => cpu_or_zero(a.cpu).total_cmp(&cpu_or_zero(b.cpu)),
            SortKey::Memory => a.proc.rss.cmp(&b.proc.rss),
            SortKey::Time => a.proc.cpu_time().cmp(&b.proc.cpu_time()),
        };
        let ordering = if descending {
            ordering.reverse()
        } else {
            ordering
        };
        // The tie-break is about frame-to-frame stability, not about the
        // column's direction, so it is not reversed with it.
        ordering.then_with(|| a.proc.pid.cmp(&b.proc.pid))
    });
}

fn cpu_or_zero(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v }
}

fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    fn fold(s: &str) -> impl Iterator<Item = char> + '_ {
        s.chars().flat_map(char::to_lowercase)
    }
    fold(a).cmp(fold(b))
}
