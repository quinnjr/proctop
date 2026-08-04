//! Cursor and scroll position for the process table.
//!
//! Kept as plain data with no dependency on the renderer, so the awkward
//! cases — an empty list, a terminal too short for any rows, a list that
//! shrinks under the cursor — can be tested directly.

use std::ops::Range;

/// Which row is selected, and where the visible window starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub index: usize,
    pub offset: usize,
}

impl Selection {
    /// Move the cursor by `delta` rows, scrolling only as far as needed to
    /// keep it on screen. Stops at both ends rather than wrapping.
    pub fn move_by(&mut self, delta: isize, len: usize, height: usize) {
        if len == 0 {
            *self = Selection::default();
            return;
        }
        let last = (len - 1) as isize;
        self.index = (self.index as isize).saturating_add(delta).clamp(0, last) as usize;
        self.scroll_into_view(len, height);
    }

    /// Jump to the first row.
    pub fn to_top(&mut self) {
        *self = Selection::default();
    }

    /// Jump to the last row.
    pub fn to_bottom(&mut self, len: usize, height: usize) {
        if len == 0 {
            *self = Selection::default();
            return;
        }
        self.index = len - 1;
        self.scroll_into_view(len, height);
    }

    /// Pull the cursor and window back into range after the list changed
    /// size beneath them.
    ///
    /// Processes exit between samples, so the list routinely shrinks under a
    /// cursor that was valid a moment ago; left alone it would point past
    /// the end and scroll to a blank window.
    pub fn clamp(&mut self, len: usize, height: usize) {
        if len == 0 {
            *self = Selection::default();
            return;
        }
        self.index = self.index.min(len - 1);
        self.scroll_into_view(len, height);
    }

    /// The range of rows to render.
    ///
    /// Callers slice the process list with this *before* building any row,
    /// so the cost of a frame is proportional to the visible rows rather
    /// than to the number of processes.
    pub fn visible(&self, len: usize, height: usize) -> Range<usize> {
        let start = self.offset.min(len);
        let end = start.saturating_add(height).min(len);
        start..end
    }

    /// Move the window the minimum distance that puts `index` back on
    /// screen, so scrolling tracks the cursor instead of recentering it.
    fn scroll_into_view(&mut self, len: usize, height: usize) {
        if height == 0 {
            self.offset = 0;
            return;
        }
        // Never leave blank rows below a full list.
        let max_offset = len.saturating_sub(height);

        if self.index < self.offset {
            self.offset = self.index;
        } else if self.index >= self.offset + height {
            self.offset = self.index + 1 - height;
        }
        self.offset = self.offset.min(max_offset);
    }
}
