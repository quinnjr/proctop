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

    fn admits(&self, row: &ProcRow) -> bool {
        if self.hide_kernel_threads && is_kernel_thread(row) {
            return false;
        }
        if let Some(user) = &self.user
            && &row.user != user
        {
            return false;
        }
        if !self.query.is_empty() && !matches_query(row, &self.query) {
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
    rows.into_iter().filter(|row| filter.admits(row)).collect()
}

/// Match the command name case-insensitively, or the PID exactly.
///
/// Case-insensitivity matters more than it looks: typing `firefox` and
/// getting nothing because the process is named `Firefox` reads as a broken
/// search rather than a precise one.
fn matches_query(row: &ProcRow, query: &str) -> bool {
    if row.proc.pid.to_string() == query {
        return true;
    }
    contains_ignore_case(&row.proc.name, query)
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// A kernel thread has no address space of its own, which is how htop
/// distinguishes it from a userspace process.
fn is_kernel_thread(row: &ProcRow) -> bool {
    row.proc.vsize == 0
}
