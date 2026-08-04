//! Predicates over the process list.

use crate::model::ProcRow;

/// Everything currently narrowing the table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// Matched against the command name and the PID.
    pub query: String,
    pub user: Option<String>,
    pub hide_kernel_threads: bool,
}

impl Filter {
    /// Whether this filter would remove anything.
    ///
    /// An empty query means "no filter", not "match nothing" — otherwise
    /// backspacing the last character of a search empties the table.
    pub fn is_active(&self) -> bool {
        !self.query.is_empty() || self.user.is_some() || self.hide_kernel_threads
    }

    fn admits(&self, row: &ProcRow, needle: &str, pid: Option<i32>) -> bool {
        if self.hide_kernel_threads && is_kernel_thread(row) {
            return false;
        }
        if let Some(user) = &self.user
            && row.user.as_ref() != user.as_str()
        {
            return false;
        }
        if !self.query.is_empty() && !matches_query(row, needle, pid) {
            return false;
        }
        true
    }
}

/// Narrow `rows` to those the filter admits.
pub fn apply(rows: Vec<ProcRow>, filter: &Filter) -> Vec<ProcRow> {
    if !filter.is_active() {
        return rows;
    }
    // The query is lowercased once here rather than once per row: this runs
    // over the whole process list on every keystroke of an incremental
    // search, so per-row work on the needle is multiplied by both.
    let needle = filter.query.to_lowercase();
    let pid = filter.query.parse::<i32>().ok();
    rows.into_iter()
        .filter(|row| filter.admits(row, &needle, pid))
        .collect()
}

/// Match the command name case-insensitively, or the PID as a number.
///
/// The PID comparison is numeric rather than textual, so `4`, `004` and
/// `+4` all find pid 4 — more forgiving than the string form it replaced,
/// and it costs no allocation per row.
///
/// Case-insensitivity matters more than it looks: typing `firefox` and
/// getting nothing because the process is named `Firefox` reads as a broken
/// search rather than a precise one.
fn matches_query(row: &ProcRow, needle: &str, pid: Option<i32>) -> bool {
    // Compare the parsed number rather than formatting the pid, which would
    // allocate for every row.
    if pid == Some(row.proc.pid) {
        return true;
    }
    contains_ignore_case(&row.proc.name, needle)
}

/// `needle` must already be lowercase.
fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    // Command names are short and overwhelmingly ASCII, and this runs over
    // every row on every keystroke of an incremental search — so the common
    // case compares in place rather than lowercasing into a fresh String.
    if haystack.is_ascii() && needle.is_ascii() {
        let (haystack, needle) = (haystack.as_bytes(), needle.as_bytes());
        if needle.len() > haystack.len() {
            return false;
        }
        return haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle));
    }
    haystack.to_lowercase().contains(needle)
}

/// A kernel thread has no address space of its own, which is how htop
/// distinguishes it from a userspace process.
fn is_kernel_thread(row: &ProcRow) -> bool {
    row.proc.vsize == 0
}
