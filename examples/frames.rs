//! Renders rtop headlessly and prints each screen as plain text.
//!
//! For eyeballing layout without a terminal: column alignment, whether
//! anything overflows, what an overlay covers. Colors do not survive
//! `frame_text`, so this shows structure only.

use ntui::testing::TestTerminal;
use ntui::{KeyCode, element};
use rtop::ui::app::App;

const WIDTH: u16 = 130;
const HEIGHT: u16 = 45;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut terminal = TestTerminal::new(WIDTH, HEIGHT, element!(App)).expect("should mount");

    // Two samples, so the CPU and throughput figures are real rather than
    // the zeros the first frame necessarily shows.
    for _ in 0..80 {
        terminal.tick().await.expect("should tick");
        if !terminal.frame_text().contains("Tasks: 0,") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(1600)).await;
    terminal.tick().await.expect("should tick");

    show("Processes", &terminal);

    for (label, keys) in [
        ("I/O", vec![KeyCode::Char('2')]),
        ("Sensors", vec![KeyCode::Char('3')]),
        ("Tree view", vec![KeyCode::Char('1'), KeyCode::Char('t')]),
        ("Help", vec![KeyCode::Char('t'), KeyCode::Char('?')]),
        ("Detail", vec![KeyCode::Esc, KeyCode::Enter]),
        ("Kill", vec![KeyCode::Esc, KeyCode::Char('d')]),
    ] {
        for key in keys {
            terminal.send_key(key).expect("should accept input");
        }
        // Sensors are only read while their tab is showing, so give the
        // sampler a moment after switching before capturing the frame.
        for _ in 0..40 {
            terminal.tick().await.expect("should tick");
            if !terminal.frame_text().contains("Reading sensors") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        show(label, &terminal);
    }
}

fn show(label: &str, terminal: &TestTerminal) {
    println!(
        "\n╔══ {label} {}",
        "═".repeat(70_usize.saturating_sub(label.len()))
    );
    for line in terminal.frame_text().lines() {
        println!("║{}", line.trim_end());
    }
}
