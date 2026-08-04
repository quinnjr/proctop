use ntui::element;
use ntui::testing::{TestTerminal, render_once};
use rtop::model::{MemInfo, Proc, ProcRow, Sample};
use rtop::sort::SortKey;
use rtop::ui::Selection;
use rtop::ui::Shared;
use rtop::ui::meters::{Meters, MetersProps};
use rtop::ui::palette::Palette;
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
            user: "joseph".into(),
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
    // `render_once` is ntui's harness for exactly this.
    let built = render_once::<ProcessTable>(&ProcessTableProps {
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

// ---------- meter layout ----------

/// The number of terminal rows `Meters` actually renders: its tallest meter
/// column, plus the summary line beneath.
fn rendered_meter_rows(cores: usize, width: u16, max_rows: Option<u16>) -> u16 {
    let sample = Sample {
        cores: vec![Default::default(); cores],
        ..Sample::default()
    };
    let el = render_once::<Meters>(&MetersProps {
        sample: Shared::new(sample),
        palette: Palette::default(),
        width,
        max_rows,
    });

    // A budget with no room for a meter renders no header at all, which is
    // a fragment rather than a view.
    let ntui::Node::View { children, .. } = el.node else {
        return 0;
    };
    // children = [row of meter columns, summary line]
    let ntui::Node::View { children: cols, .. } = &children[0].node else {
        panic!("expected the meter row")
    };
    let tallest = cols
        .iter()
        .map(|c| match &c.node {
            ntui::Node::View { children, .. } => children.len(),
            _ => 0,
        })
        .max()
        .unwrap_or(0);
    (tallest + 1) as u16
}

#[test]
fn the_reported_header_height_matches_what_is_rendered() {
    // `App` subtracts this from the terminal height to size the body. If the
    // two disagree, the tab bar and status bar are pushed off the bottom by
    // exactly the difference.
    // Swept across budgets, not just the unbounded one: the two branches
    // the `Option<u16>` change introduced are both budget-gated, so a
    // `None`-only sweep leaves `height` and the renderer free to disagree
    // exactly where they used to.
    for cores in [0usize, 1, 2, 4, 8, 16, 32, 64] {
        for width in [40u16, 60, 80, 120, 200] {
            for budget in [None, Some(0), Some(1), Some(2), Some(3), Some(6), Some(10)] {
                assert_eq!(
                    rtop::ui::meters::height(cores, width, budget),
                    rendered_meter_rows(cores, width, budget),
                    "cores={cores} width={width} budget={budget:?}"
                );
            }
        }
    }
}

#[test]
fn a_budget_with_no_room_for_a_meter_renders_no_header_at_all() {
    // `Some(0)` used to be spelled `0`, which meant "unbounded" — so the
    // one case with no room was read as the one case with unlimited room.
    for budget in [Some(0), Some(1)] {
        assert_eq!(rtop::ui::meters::height(16, 120, budget), 0, "{budget:?}");
        assert_eq!(rendered_meter_rows(16, 120, budget), 0, "{budget:?}");
    }
    assert!(rtop::ui::meters::height(16, 120, Some(4)) > 0);
}

#[test]
fn the_header_never_grows_past_its_row_budget() {
    // A narrow pane cannot fit many columns, and without a cap the meters
    // grow downward instead — 32 cores in a 50-column pane wanted 35 rows,
    // which left no room for the process table, the tab bar, or the status
    // bar. Dropping meters past the cap is the better failure.
    for cores in [1usize, 8, 32, 64, 256] {
        for width in [20u16, 40, 80, 200] {
            let height = rtop::ui::meters::height(cores, width, None);
            assert!(
                height <= rtop::ui::meters::MAX_ROWS as u16 + 1,
                "cores={cores} width={width} wanted {height} rows"
            );
        }
    }
}

#[test]
fn a_terminal_too_narrow_for_one_meter_column_still_reports_a_sane_height() {
    let height = rtop::ui::meters::height(32, 1, None);

    assert!(height >= 2, "at least one meter and the summary");
    assert!(height <= rtop::ui::meters::MAX_ROWS as u16 + 1);
}

// ---------- view row budgets ----------

use ntui::Shared as NShared;
use rtop::model::{IoRate, Sensor, SensorKind};
use rtop::ui::io::{DiskView, DiskViewProps};
use rtop::ui::sensors::{SensorView, SensorViewProps};

fn count_rows(el: ntui::Element) -> usize {
    match el.node {
        ntui::Node::View { children, .. } => children.len(),
        _ => 0,
    }
}

#[test]
fn the_sensors_view_fits_its_row_budget_across_every_kind() {
    // The budget was applied per kind, so three kinds meant three times the
    // rows the tab has room for and every fan reading fell below the fold.
    let sensors: Vec<Sensor> = [
        SensorKind::Temperature,
        SensorKind::Fan,
        SensorKind::Battery,
    ]
    .into_iter()
    .flat_map(|kind| {
        (0..10).map(move |i| Sensor {
            chip: "chip".into(),
            label: format!("{kind:?}-{i}"),
            kind,
            value: 40.0,
        })
    })
    .collect();

    let sample = Sample {
        sensors: Some(NShared::new(sensors)),
        ..Sample::default()
    };

    for height in [1u16, 2, 4, 8, 20] {
        let rows = count_rows(render_once::<SensorView>(&SensorViewProps {
            sample: Shared::new(sample.clone()),
            palette: Palette::default(),
            height,
        }));
        assert!(
            rows <= height as usize,
            "height={height} produced {rows} rows"
        );
    }
}

fn io_sample(disks: usize, nets: usize) -> Sample {
    let rate = |name: String| IoRate {
        name,
        read_per_sec: 1_000,
        write_per_sec: 1_000,
    };
    Sample {
        disks: Some((0..disks).map(|i| rate(format!("disk{i}"))).collect()),
        nets: Some((0..nets).map(|i| rate(format!("net{i}"))).collect()),
        ..Sample::default()
    }
}

#[test]
fn the_disk_view_fits_its_row_budget() {
    for height in [1u16, 2, 4, 6, 10, 20] {
        for (disks, nets) in [(0, 0), (1, 8), (8, 1), (20, 20)] {
            let rows = count_rows(render_once::<DiskView>(&DiskViewProps {
                sample: Shared::new(io_sample(disks, nets)),
                palette: Palette::default(),
                height,
            }));
            assert!(
                rows <= height as usize,
                "height={height} disks={disks} nets={nets} produced {rows} rows"
            );
        }
    }
}

#[test]
fn the_disk_view_gives_its_whole_budget_to_the_list() {
    // The tab holds one section now, so a long device list should reach the
    // bottom of the pane rather than stopping at a share of it.
    let many_disks = render_once::<DiskView>(&DiskViewProps {
        sample: Shared::new(io_sample(20, 8)),
        palette: Palette::default(),
        height: 12,
    });

    assert_eq!(
        count_rows(many_disks),
        12,
        "the whole budget should be used"
    );
}

#[test]
fn the_disk_view_does_not_show_interfaces() {
    // Interfaces moved to the Network tab, beside the sockets that explain
    // them. Showing them here too means the same rows on two tabs.
    let t = TestTerminal::new(
        60,
        20,
        element!(DiskView(
            sample: Shared::new(io_sample(2, 4)),
            palette: Palette::default(),
            height: 20u16,
        )),
    )
    .expect("should render");

    let text = t.frame_text();
    assert!(text.contains("Disks"), "disks belong here:\n{text}");
    for absent in ["net0", "RX", "TX"] {
        assert!(
            !text.contains(absent),
            "{absent} belongs to the Network tab:\n{text}"
        );
    }
}

#[test]
fn an_unreadable_counter_file_reads_as_unavailable_not_as_idle() {
    // A restricted container reporting its disks as idle is a lie.
    let unreadable = TestTerminal::new(
        60,
        20,
        element!(DiskView(
            sample: Shared::new(Sample { disks: None, ..Sample::default() }),
            palette: Palette::default(),
            height: 20u16,
        )),
    )
    .expect("should render");

    let text = unreadable.frame_text();
    assert!(text.contains("unavailable"), "disks should say so:\n{text}");
    assert!(!text.contains("idle"), "None is not idle:\n{text}");

    // And the other side of the distinction: read them, nothing moving.
    let quiet = TestTerminal::new(
        60,
        20,
        element!(DiskView(
            sample: Shared::new(Sample { disks: Some(Vec::new()), ..Sample::default() }),
            palette: Palette::default(),
            height: 20u16,
        )),
    )
    .expect("should render");

    let text = quiet.frame_text();
    assert!(text.contains("idle"), "these genuinely are idle:\n{text}");
    assert!(!text.contains("unavailable"), "they were read:\n{text}");
}

#[test]
fn exactly_one_header_is_marked_for_each_sort_column() {
    // The marker's job is to name the column driving the order. Two columns
    // claiming the same key leaves it naming neither.
    for key in [
        SortKey::Pid,
        SortKey::Name,
        SortKey::Cpu,
        SortKey::Memory,
        SortKey::Time,
    ] {
        let marked = rtop::ui::table::COLUMNS
            .iter()
            .filter(|c| c.sort == Some(key))
            .count()
            // The command column is laid out separately, but it is marked
            // the same way and counts the same.
            + usize::from(rtop::ui::table::COMMAND_SORT == Some(key));
        assert_eq!(marked, 1, "{key:?} is claimed by {marked} columns");
    }
}

// ---------- network tab ----------

use rtop::model::{ListeningSocket, Protocol, Socket};
use rtop::ui::network::{NetworkView, NetworkViewProps};

fn socket(port: u16, attributed: bool) -> ListeningSocket {
    ListeningSocket {
        socket: Socket {
            protocol: Protocol::Tcp,
            local: format!("0.0.0.0:{port}").parse().unwrap(),
            uid: 0,
            inode: port as u64,
            accept_queue: Some(0),
        },
        user: "root".into(),
        process: attributed.then(|| (1, "init".into())),
    }
}

fn net_sample(nets: Option<usize>, sockets: Option<Vec<ListeningSocket>>) -> Sample {
    Sample {
        nets: nets.map(|n| {
            (0..n)
                .map(|i| IoRate {
                    name: format!("net{i}"),
                    read_per_sec: 1_000,
                    write_per_sec: 1_000,
                })
                .collect()
        }),
        sockets: sockets.map(NShared::new),
        ..Sample::default()
    }
}

#[test]
fn the_network_view_fits_its_row_budget() {
    // The sweep the Disk and Sensors views have had all along. Without it
    // the listening section overflowed by exactly one row whenever the
    // budget left no room past its own heading — which is every first frame
    // on the tab, since the sockets have not been read yet.
    let lists = [
        None,
        Some(Vec::new()),
        Some((0..3).map(|i| socket(80 + i, true)).collect()),
        Some((0..40).map(|i| socket(80 + i, i % 3 == 0)).collect()),
    ];

    for height in [0u16, 1, 2, 3, 4, 5, 6, 10, 20] {
        for nets in [None, Some(0), Some(1), Some(8)] {
            for sockets in &lists {
                let rows = count_rows(render_once::<NetworkView>(&NetworkViewProps {
                    sample: Shared::new(net_sample(nets, sockets.clone())),
                    palette: Palette::default(),
                    height,
                    selection: Selection::default(),
                }));
                assert!(
                    rows <= height as usize,
                    "height={height} nets={nets:?} sockets={:?} produced {rows} rows",
                    sockets.as_ref().map(Vec::len)
                );
            }
        }
    }
}

#[test]
fn the_network_view_gives_the_listeners_everything_the_interfaces_left() {
    // The `<= height` sweep alone passes if the section renders a header
    // and no rows at all, so it cannot catch the guard being widened.
    let sockets: Vec<ListeningSocket> = (0..40).map(|i| socket(80 + i, true)).collect();

    for height in [6u16, 8, 10, 20, 30] {
        let rows = count_rows(render_once::<NetworkView>(&NetworkViewProps {
            sample: Shared::new(net_sample(Some(1), Some(sockets.clone()))),
            palette: Palette::default(),
            height,
            selection: Selection::default(),
        }));
        assert_eq!(
            rows, height as usize,
            "height={height}: the pane should be exactly full"
        );
    }
}

#[test]
fn the_network_view_says_it_is_still_reading_rather_than_unavailable() {
    // `None` here means "this tab just opened", not "the kernel denied it" —
    // sockets are only read while the tab shows. Calling that unavailable is
    // the lie the disk view used to tell.
    let t = TestTerminal::new(
        70,
        20,
        element!(NetworkView(
            sample: Shared::new(net_sample(Some(1), None)),
            palette: Palette::default(),
            height: 20u16,
            selection: Selection::default(),
        )),
    )
    .expect("should render");

    let text = t.frame_text();
    assert!(text.contains("reading sockets"), "{text}");
    assert!(!text.contains("unavailable"), "{text}");
}

#[test]
fn the_attribution_footer_appears_only_when_a_row_is_unattributed() {
    let render = |sockets: Vec<ListeningSocket>| {
        TestTerminal::new(
            80,
            20,
            element!(NetworkView(
                sample: Shared::new(net_sample(Some(1), Some(sockets))),
                palette: Palette::default(),
                height: 20u16,
                selection: Selection::default(),
            )),
        )
        .expect("should render")
        .frame_text()
    };

    let all_known = render(vec![socket(80, true), socket(443, true)]);
    assert!(
        !all_known.contains("run as root"),
        "nothing to explain:\n{all_known}"
    );

    // Count and plural only: which explanation is offered depends on the
    // uid the suite runs as, and `attribution_note`'s own unit tests pin
    // both wordings deterministically. The first version of this test
    // locked in "1 sockets".
    let one_unknown = render(vec![socket(80, true), socket(443, false)]);
    assert!(
        one_unknown.contains("1 socket "),
        "singular:\n{one_unknown}"
    );

    let two_unknown = render(vec![socket(80, false), socket(443, false)]);
    assert!(two_unknown.contains("2 sockets "), "plural:\n{two_unknown}");
}
