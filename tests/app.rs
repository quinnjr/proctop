//! Whole-app tests. These mount the real root component, which samples the
//! live machine — the assertions are deliberately about wiring and input,
//! not about any particular figure.

#![cfg(target_os = "linux")]

use ntui::testing::TestTerminal;
use ntui::{KeyCode, element};
use rtop::ui::app::{App, AppProps};

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

/// An app that has taken at least one sample.
async fn sampled() -> TestTerminal {
    let mut t = app();
    tick_until(&mut t, |text| !text.contains("Tasks: 0,")).await;
    t
}

#[tokio::test]
async fn mounts_and_renders_its_chrome() {
    let mut t = app();
    t.tick().await.expect("should tick");

    let text = t.frame_text();

    assert!(text.contains("PID"), "missing the table header:\n{text}");
    assert!(text.contains("Tasks"), "missing the summary line:\n{text}");
    assert!(text.contains("q quit"), "missing the status bar:\n{text}");
    assert!(text.contains("1:Processes"), "missing the tab bar:\n{text}");
}

#[tokio::test]
async fn shows_the_running_machine_once_a_sample_lands() {
    let mut t = app();

    // Not asserting on any particular process: sorted by CPU, which
    // processes are on screen depends entirely on what the machine is doing.
    let text = tick_until(&mut t, |text| !text.contains("Tasks: 0,")).await;

    assert!(text.contains("joseph") || text.contains("root"), "{text}");
    // The leading space matters: the status bar opens with one, and without
    // it this matches any count ending in a zero ("790 procs").
    assert!(
        !text.contains(" 0 procs"),
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
    // The header alone is taller than the terminal, so the body is asked for
    // a negative number of rows.
    let mut t = TestTerminal::new(40, 3, element!(App)).expect("should mount");

    t.tick().await.expect("should tick");
    t.send_key(KeyCode::Char('j')).expect("should accept input");
    t.send_key(KeyCode::Char('G')).expect("should accept input");
    t.send_key(KeyCode::Tab).expect("should accept input");

    assert!(!t.exited(), "still running");
}

#[tokio::test]
async fn cycles_the_sort_column() {
    let mut t = app();
    t.tick().await.expect("should tick");
    assert!(t.frame_text().contains("CPU% desc"), "{}", t.frame_text());

    t.send_key(KeyCode::Char('>')).expect("should accept input");

    assert!(
        t.frame_text().contains("RES desc"),
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

#[tokio::test]
async fn opens_and_closes_the_help_overlay() {
    let mut t = app();
    t.tick().await.expect("should tick");

    t.send_key(KeyCode::Char('?')).expect("should accept input");
    let text = t.frame_text();
    assert!(text.contains("rtop — keys"), "{text}");
    assert!(text.contains("incremental search"), "{text}");

    t.send_key(KeyCode::Esc).expect("should accept input");

    assert!(!t.frame_text().contains("rtop — keys"));
    assert!(!t.exited(), "Esc should close the overlay, not quit");
}

#[tokio::test]
async fn switches_to_the_io_tab() {
    let mut t = sampled().await;

    t.send_key(KeyCode::Char('2')).expect("should accept input");

    let text = t.frame_text();
    assert!(text.contains("Disks"), "{text}");
    assert!(text.contains("Network"), "{text}");
    assert!(text.contains("DEVICE"), "{text}");
}

#[tokio::test]
async fn switches_to_the_sensors_tab() {
    let mut t = sampled().await;

    t.send_key(KeyCode::Char('3')).expect("should accept input");

    // Hardware readings are only taken while this tab is showing, so the
    // frame immediately after the switch says so rather than claiming the
    // machine has no sensors.
    assert!(
        t.frame_text().contains("Reading sensors"),
        "{}",
        t.frame_text()
    );

    let text = tick_until(&mut t, |text| !text.contains("Reading sensors")).await;

    // A machine with no hwmon says so rather than rendering an empty table.
    assert!(
        text.contains("CHIP") || text.contains("No sensors available"),
        "{text}"
    );
}

#[tokio::test]
async fn does_not_read_sensors_while_the_process_tab_is_showing() {
    // The hwmon files on a desktop cost more to read than the whole rest of
    // a sample; paying that for a tab nobody has open is what put rtop over
    // its CPU budget.
    let t = sampled().await;

    assert!(!t.frame_text().contains("CHIP"));
    assert!(!t.frame_text().contains("No sensors available"));
}

#[tokio::test]
async fn searching_narrows_the_table() {
    let mut t = sampled().await;
    let before = row_count(&t.frame_text());

    t.send_key(KeyCode::Char('/')).expect("should accept input");
    for c in "systemd".chars() {
        t.send_key(KeyCode::Char(c)).expect("should accept input");
    }

    let text = t.frame_text();
    assert!(text.contains("/systemd_"), "missing the prompt:\n{text}");
    assert!(
        row_count(&text) < before,
        "the table should have narrowed: {before} -> {}",
        row_count(&text)
    );
}

#[tokio::test]
async fn escape_clears_a_search_before_it_quits() {
    let mut t = sampled().await;
    t.send_key(KeyCode::Char('/')).expect("should accept input");
    t.send_key(KeyCode::Char('z')).expect("should accept input");
    t.send_key(KeyCode::Enter).expect("should accept input");

    t.send_key(KeyCode::Esc).expect("should accept input");
    assert!(!t.exited(), "the first Esc clears the filter");

    t.send_key(KeyCode::Esc).expect("should accept input");
    assert!(t.exited(), "the second quits");
}

#[tokio::test]
async fn the_command_line_sets_the_sort_column() {
    let mut t = sampled().await;

    t.send_key(KeyCode::Char(':')).expect("should accept input");
    for c in "sort pid".chars() {
        t.send_key(KeyCode::Char(c)).expect("should accept input");
    }
    t.send_key(KeyCode::Enter).expect("should accept input");

    assert!(t.frame_text().contains("PID desc"), "{}", t.frame_text());
}

#[tokio::test]
async fn opens_the_detail_pane_for_the_selected_process() {
    let mut t = sampled().await;

    t.send_key(KeyCode::Enter).expect("should accept input");

    let text = t.frame_text();
    assert!(text.contains("Resident"), "{text}");
    assert!(text.contains("CPU time"), "{text}");
}

#[tokio::test]
async fn the_kill_dialog_asks_before_signalling_anything() {
    let mut t = sampled().await;

    t.send_key(KeyCode::Char('d')).expect("should accept input");

    let text = t.frame_text();
    assert!(text.contains("Send signal"), "{text}");
    assert!(text.contains("SIGTERM"), "{text}");
    assert!(text.contains("Esc cancel"), "{text}");

    t.send_key(KeyCode::Esc).expect("should accept input");
    assert!(!t.frame_text().contains("Send signal"));
}

#[tokio::test]
async fn honours_the_configured_sort_column() {
    let config =
        rtop::config::Config::parse("[processes]\nsort_by = \"pid\"").expect("should parse");
    let mut t = TestTerminal::new(120, 40, element!(App(config: config))).expect("should mount");

    t.tick().await.expect("should tick");

    assert!(t.frame_text().contains("PID desc"), "{}", t.frame_text());
}

#[tokio::test]
async fn honours_a_configured_theme() {
    // Colors are not visible through `frame_text`, so this only checks that
    // a non-default palette mounts and renders.
    let palette = rtop::ui::palette::Palette::named("mono").expect("bundled");
    let mut t = TestTerminal::new(120, 40, element!(App(palette: palette))).expect("should mount");

    t.tick().await.expect("should tick");

    assert!(t.frame_text().contains("PID"));
}

/// Rows in the table, counted from the frame: lines that begin with a pid.
fn row_count(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed
                .split_whitespace()
                .next()
                .is_some_and(|word| word.parse::<u32>().is_ok())
                && !trimmed.starts_with("Tasks")
        })
        .count()
}
