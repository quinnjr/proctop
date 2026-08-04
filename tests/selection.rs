//! rtop's contract tests for the cursor it upstreamed to ntui.
//!
//! The implementation lives in `ntui::ListSelection` now; these stay as the
//! downstream statement of what rtop needs from it, so a change upstream
//! that breaks the process table fails here too.

use rtop::ui::Selection;

#[test]
fn moves_down_within_the_visible_window_without_scrolling() {
    let mut s = Selection::default();

    s.move_by(1, 100, 10);

    assert_eq!(s.index, 1);
    assert_eq!(s.offset, 0, "the row is already on screen");
}

#[test]
fn scrolls_by_one_when_the_selection_leaves_the_bottom_edge() {
    let mut s = Selection::default();

    s.move_by(10, 100, 10);

    assert_eq!(s.index, 10);
    assert_eq!(s.offset, 1, "just enough to bring row 10 into view");
}

#[test]
fn scrolls_back_when_the_selection_leaves_the_top_edge() {
    let mut s = Selection {
        index: 50,
        offset: 45,
    };

    s.move_by(-10, 100, 10);

    assert_eq!(s.index, 40);
    assert_eq!(s.offset, 40);
}

#[test]
fn stops_at_the_first_row_rather_than_wrapping() {
    let mut s = Selection::default();

    s.move_by(-5, 100, 10);

    assert_eq!(s.index, 0);
    assert_eq!(s.offset, 0);
}

#[test]
fn stops_at_the_last_row_rather_than_wrapping() {
    let mut s = Selection::default();

    s.move_by(500, 100, 10);

    assert_eq!(s.index, 99);
    assert_eq!(s.offset, 90);
}

#[test]
fn survives_an_empty_process_list() {
    // The list is empty for one frame at startup, before the first sample.
    let mut s = Selection::default();

    s.move_by(1, 0, 10);

    assert_eq!(s.index, 0);
    assert_eq!(s.offset, 0);
    assert_eq!(s.visible(0, 10), 0..0);
}

#[test]
fn survives_a_terminal_too_short_to_show_any_rows() {
    let mut s = Selection::default();

    s.move_by(5, 100, 0);

    assert_eq!(s.visible(100, 0), 0..0);
}

#[test]
fn jumps_to_the_last_row_with_the_window_at_the_end() {
    let mut s = Selection::default();

    s.to_end(100, 10);

    assert_eq!(s.index, 99);
    assert_eq!(s.offset, 90);
    assert_eq!(s.visible(100, 10), 90..100);
}

#[test]
fn jumps_back_to_the_first_row() {
    let mut s = Selection {
        index: 80,
        offset: 75,
    };

    s.to_start();

    assert_eq!(s.index, 0);
    assert_eq!(s.offset, 0);
}

#[test]
fn keeps_the_selection_on_screen_when_the_list_shrinks_underneath_it() {
    // Processes exit between samples. A selection left pointing past the end
    // of the list would render nothing and scroll to a blank window.
    let mut s = Selection {
        index: 90,
        offset: 85,
    };

    s.clamp(20, 10);

    assert_eq!(s.index, 19);
    assert_eq!(s.offset, 10);
    assert_eq!(s.visible(20, 10), 10..20);
}

#[test]
fn shows_the_whole_list_when_it_is_shorter_than_the_window() {
    let s = Selection::default();

    assert_eq!(s.visible(3, 10), 0..3);
}
