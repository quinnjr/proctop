//! Whole-app tests. These mount the real root component, which samples the
//! live machine — the assertions are deliberately about wiring and input,
//! not about any particular figure.

#![cfg(target_os = "linux")]

use ntui::testing::TestTerminal;
use ntui::{KeyCode, element};
use rtop::ui::app::App;

fn app() -> TestTerminal {
    TestTerminal::new(120, 40, element!(App)).expect("should mount")
}

/// Tick until `done` holds, or give up.
///
/// Sampling runs on `spawn_blocking`, so whether it has landed after any
/// single tick is a race. Polling keeps these tests from depending on how
/// the runtime happened to schedule that thread.
async fn tick_until(t: &mut TestTerminal, done: impl Fn(&str) -> bool) -> String {
    for _ in 0..100 {
        t.tick().await.expect("should tick");
        let text = t.frame_text();
        if done(&text) {
            return text;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("condition never held:\n{}", t.frame_text());
}

#[tokio::test]
async fn mounts_and_renders_its_chrome() {
    let mut t = app();
    t.tick().await.expect("should tick");

    let text = t.frame_text();

    assert!(text.contains("PID"), "missing the table header:\n{text}");
    assert!(text.contains("Tasks"), "missing the summary line:\n{text}");
    assert!(text.contains("q quit"), "missing the status bar:\n{text}");
}

#[tokio::test]
async fn shows_the_running_machine_once_a_sample_lands() {
    let mut t = app();

    // Not asserting on any particular process: sorted by CPU, which
    // processes are on screen depends entirely on what the machine is doing.
    let text = tick_until(&mut t, |text| !text.contains("Tasks: 0,")).await;

    assert!(text.contains("joseph") || text.contains("root"), "{text}");
    // The leading space matters: the status bar opens with one, and without
    // it this matches any count ending in a zero ("790 sorted by").
    assert!(
        !text.contains(" 0 sorted by"),
        "the table should have rows:\n{text}"
    );
}

#[tokio::test]
async fn quits_on_q() {
    let mut t = app();
    t.tick().await.expect("should tick");
    assert!(!t.exited());

    t.send_key(KeyCode::Char('q')).expect("should accept input");

    assert!(t.exited());
}

#[tokio::test]
async fn quits_on_escape() {
    let mut t = app();
    t.tick().await.expect("should tick");

    t.send_key(KeyCode::Esc).expect("should accept input");

    assert!(t.exited());
}

#[tokio::test]
async fn survives_a_terminal_too_small_for_any_rows() {
    // The header alone is taller than the terminal, so the table is asked
    // for a negative number of rows.
    let mut t = TestTerminal::new(40, 3, element!(App)).expect("should mount");

    t.tick().await.expect("should tick");
    t.send_key(KeyCode::Char('j')).expect("should accept input");
    t.send_key(KeyCode::Char('G')).expect("should accept input");

    assert!(!t.exited(), "still running");
}

#[tokio::test]
async fn cycles_the_sort_column() {
    let mut t = app();
    t.tick().await.expect("should tick");
    assert!(t.frame_text().contains("sorted by CPU%"));

    t.send_key(KeyCode::Char('>')).expect("should accept input");

    assert!(
        t.frame_text().contains("sorted by RES"),
        "'>' should advance to the next column:\n{}",
        t.frame_text()
    );
}

#[tokio::test]
async fn reverses_the_sort_direction() {
    let mut t = app();
    t.tick().await.expect("should tick");
    assert!(t.frame_text().contains("desc"));

    t.send_key(KeyCode::Char('I')).expect("should accept input");

    assert!(t.frame_text().contains("asc"));
}
