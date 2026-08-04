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
        ("Disk", vec![KeyCode::Char('2')]),
        ("Network", vec![KeyCode::Char('3')]),
        ("Sensors", vec![KeyCode::Char('4')]),
        ("Tree view", vec![KeyCode::Char('1'), KeyCode::Char('t')]),
        ("Help", vec![KeyCode::Char('t'), KeyCode::Char('?')]),
        ("Detail", vec![KeyCode::Esc, KeyCode::Enter]),
        (
            "Kill",
            vec![KeyCode::Esc, KeyCode::Char('d'), KeyCode::Char('d')],
        ),
    ] {
        for key in keys {
            terminal.send_key(key).expect("should accept input");
        }
        // Sensors and sockets are read only while their own tab is
        // showing, and the sampling loop picks that up on its next tick —
        // so a switch can wait a full refresh interval before the data
        // lands. Poll past that rather than capturing the placeholder.
        for _ in 0..120 {
            terminal.tick().await.expect("should tick");
            let text = terminal.frame_text();
            let pending = text.contains("Reading sensors") || text.contains("reading sockets");
            if !pending {
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
