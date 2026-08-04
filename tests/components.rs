use ntui::element;
use ntui::testing::TestTerminal;
use rtop::model::{MemInfo, Proc, ProcRow, Sample};
use rtop::sort::SortKey;
use rtop::ui::Shared;
use rtop::ui::meters::{Meters, MetersProps};
use rtop::ui::palette::Palette;
use rtop::ui::selection::Selection;
use rtop::ui::table::{ProcessTable, ProcessTableProps};

fn rows(n: usize) -> Vec<ProcRow> {
    (0..n)
        .map(|i| ProcRow {
            proc: Proc {
                pid: i as i32,
                name: format!("proc-{i:03}"),
                rss: 1024 * 1024,
                utime: 100,
                ..Proc::default()
            },
            cpu: 0.0,
            mem: 0.01,
            user: String::from("joseph"),
            depth: 0,
        })
        .collect()
}

fn table(rows: Vec<ProcRow>, selection: Selection, height: u16) -> TestTerminal {
    TestTerminal::new(
        100,
        height + 2,
        element!(ProcessTable(
            rows: Shared::new(rows),
            selection: selection,
            height: height,
            sort: SortKey::Cpu,
            palette: Palette::default(),
        )),
    )
    .expect("should render")
}

#[test]
fn renders_the_column_headers() {
    let t = table(rows(3), Selection::default(), 5);
    let text = t.frame_text();

    for header in ["PID", "USER", "RES", "CPU%", "MEM%", "TIME+", "Command"] {
        assert!(text.contains(header), "missing {header} in:\n{text}");
    }
}

#[test]
fn renders_a_process_row() {
    let t = table(rows(3), Selection::default(), 5);
    let text = t.frame_text();

    assert!(
        text.contains("proc-000"),
        "missing the process name:\n{text}"
    );
    assert!(text.contains("joseph"), "missing the user:\n{text}");
    assert!(text.contains("1.0M"), "missing formatted RES:\n{text}");
}

#[test]
fn builds_only_the_rows_that_are_visible() {
    // The whole point of a hand-rolled table: with 500 processes and room
    // for 5, the cost of a frame must track the window, not the list.
    //
    // This counts the elements actually constructed rather than reading the
    // rendered frame, because a frame cannot tell "never built" apart from
    // "built and then clipped" — and only the first is the property here.
    let built = ProcessTable::build(&ProcessTableProps {
        rows: Shared::new(rows(500)),
        selection: Selection::default(),
        height: 5,
        sort: SortKey::Cpu,
        palette: Palette::default(),
    });

    let ntui::Node::View { children, .. } = built.node else {
        panic!("the table should be a View");
    };
    assert_eq!(children.len(), 6, "one header plus five visible rows");
}

#[test]
fn renders_the_window_the_selection_scrolled_to() {
    let t = table(
        rows(500),
        Selection {
            index: 100,
            offset: 96,
        },
        5,
    );
    let text = t.frame_text();

    assert!(
        text.contains("proc-100"),
        "the selected row is missing:\n{text}"
    );
    assert!(text.contains("proc-096"));
    assert!(!text.contains("proc-000"));
}

#[test]
fn renders_an_empty_table_without_panicking() {
    // The list is empty for the frame between mount and the first sample.
    let t = table(Vec::new(), Selection::default(), 5);

    assert!(t.frame_text().contains("PID"));
}

#[test]
fn truncates_a_command_too_long_for_the_terminal() {
    // A process can name itself with a very long string; it must not wrap
    // onto a second line and push every later row down.
    let mut long = rows(1);
    long[0].proc.name = "x".repeat(500);

    let t = table(long, Selection::default(), 5);
    let text = t.frame_text();

    for line in text.lines() {
        assert!(line.chars().count() <= 100, "line overflowed the terminal");
    }
}

fn sample() -> Sample {
    Sample {
        cores: vec![Default::default(); 4],
        mem: MemInfo {
            total: 16 * 1024 * 1024 * 1024,
            free: 8 * 1024 * 1024 * 1024,
            swap_total: 4 * 1024 * 1024 * 1024,
            swap_free: 4 * 1024 * 1024 * 1024,
            ..MemInfo::default()
        },
        uptime: std::time::Duration::from_secs(3_661),
        procs: rows(3),
        running: 2,
        threads: 42,
        ..Sample::default()
    }
}

#[test]
fn renders_a_meter_for_every_core() {
    let t = TestTerminal::new(
        100,
        20,
        element!(Meters(sample: Shared::new(sample()), palette: Palette::default())),
    )
    .expect("should render");
    let text = t.frame_text();

    for core in 0..4 {
        assert!(text.contains(&format!("{core}")), "missing core {core}");
    }
    assert!(text.contains("Mem"), "missing the memory meter:\n{text}");
    assert!(text.contains("Swp"), "missing the swap meter:\n{text}");
}

#[test]
fn renders_the_summary_line() {
    let t = TestTerminal::new(
        100,
        20,
        element!(Meters(sample: Shared::new(sample()), palette: Palette::default())),
    )
    .expect("should render");
    let text = t.frame_text();

    assert!(text.contains("01:01:01"), "missing uptime:\n{text}");
    assert!(text.contains("Tasks"), "missing task counts:\n{text}");
    assert!(text.contains("42"), "missing the thread count:\n{text}");
}

#[test]
fn renders_meters_on_a_machine_with_no_swap() {
    let mut s = sample();
    s.mem.swap_total = 0;
    s.mem.swap_free = 0;

    let t = TestTerminal::new(
        100,
        20,
        element!(Meters(sample: Shared::new(s), palette: Palette::default())),
    )
    .expect("should render");

    assert!(t.frame_text().contains("Swp"));
}
