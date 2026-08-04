//! The header: per-core CPU meters, memory, swap, and a summary line.

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::delta::CpuUsage;
use crate::format;
use crate::model::Sample;
use crate::ui::{Shared, theme};

/// Meters are laid out in columns of at most this many rows, so a 32-core
/// machine does not push the process table off the screen.
const MAX_ROWS: usize = 8;
/// Widest a meter column is allowed to get, including its label and figure.
const METER_WIDTH: u16 = 30;

#[derive(Clone, PartialEq, Default)]
pub struct MetersProps {
    pub sample: Shared<Sample>,
}

pub struct Meters;

impl Component for Meters {
    type Props = MetersProps;

    fn render(props: &MetersProps, _hooks: &mut Hooks) -> Element {
        let sample = &props.sample;
        let cores = &sample.cores;

        // Fill each column top to bottom, the way htop does, so core 0 and
        // core 1 sit next to each other vertically rather than across the
        // screen from each other.
        let rows = rows_per_column(cores.len());
        let mut columns: Vec<Vec<Element>> = Vec::new();
        for (i, usage) in cores.iter().enumerate() {
            let column = i / rows;
            if columns.len() <= column {
                columns.push(Vec::new());
            }
            columns[column].push(cpu_meter(i, usage));
        }

        // Memory and swap ride at the end of the last column when there is
        // room, which is what keeps the header compact.
        let mut memory_column = vec![
            meter("Mem", memory_bar(sample), theme::MEM_USED),
            meter("Swp", swap_bar(sample), theme::SWAP),
        ];
        if let Some(last) = columns.last_mut()
            && last.len() + memory_column.len() <= rows
        {
            last.append(&mut memory_column);
            memory_column = Vec::new();
        }
        if !memory_column.is_empty() {
            columns.push(memory_column);
        }

        let columns: Vec<Element> = columns
            .into_iter()
            .map(|cells| {
                Element::view(
                    ViewProps {
                        flex_direction: FlexDirection::Column,
                        width: Dimension::Cells(METER_WIDTH),
                        ..Default::default()
                    },
                    cells,
                )
            })
            .collect();

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            vec![
                Element::view(
                    ViewProps {
                        flex_direction: FlexDirection::Row,
                        gap: 2,
                        ..Default::default()
                    },
                    columns,
                ),
                summary(sample),
            ],
        )
    }
}

/// How many rows each meter column holds. Columns are added rather than rows
/// once a machine has more cores than fit vertically.
fn rows_per_column(cores: usize) -> usize {
    if cores <= MAX_ROWS {
        return cores.max(1);
    }
    cores.div_ceil(cores.div_ceil(MAX_ROWS))
}

fn cpu_meter(index: usize, usage: &CpuUsage) -> Element {
    meter(
        &index.to_string(),
        format!(
            "{}{:>5.1}%",
            format::bar(usage.busy(), 16),
            usage.busy() * 100.0
        ),
        segment_color(usage),
    )
}

/// Color the bar by whatever it is mostly doing, so a machine pinned in
/// system time is distinguishable from one doing user work at a glance.
fn segment_color(usage: &CpuUsage) -> Color {
    let candidates = [
        (usage.system, theme::SYSTEM),
        (usage.nice, theme::NICE),
        (usage.irq + usage.softirq, theme::IRQ),
        (usage.iowait, theme::IOWAIT),
        (usage.user, theme::USER),
    ];
    candidates
        .iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, color)| *color)
        .unwrap_or(theme::USER)
}

fn memory_bar(sample: &Sample) -> String {
    let mem = &sample.mem;
    let used = fraction(mem.used(), mem.total);
    format!(
        "{}{:>9}/{}",
        format::bar(used, 16),
        format::bytes(mem.used()),
        format::bytes(mem.total)
    )
}

fn swap_bar(sample: &Sample) -> String {
    let mem = &sample.mem;
    let used = fraction(mem.swap_used(), mem.swap_total);
    format!(
        "{}{:>9}/{}",
        format::bar(used, 16),
        format::bytes(mem.swap_used()),
        format::bytes(mem.swap_total)
    )
}

/// A machine with no swap has a zero denominator; report an empty meter
/// rather than NaN.
fn fraction(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}

fn meter(label: &str, content: String, color: Color) -> Element {
    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            height: Dimension::Cells(1),
            ..Default::default()
        },
        vec![
            Element::view(
                ViewProps {
                    width: Dimension::Cells(4),
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: format!("{label:>3}"),
                    color: theme::LABEL,
                    weight: Weight::Bold,
                    wrap: TextWrap::Truncate,
                    ..Default::default()
                })],
            ),
            Element::text(TextProps {
                content,
                color,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
        ],
    )
}

fn summary(sample: &Sample) -> Element {
    let load = &sample.load;
    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 2,
            height: Dimension::Cells(1),
            ..Default::default()
        },
        vec![
            Element::text(TextProps {
                content: format!(
                    "Tasks: {}, {} thr, {} running",
                    sample.procs.len(),
                    sample.threads,
                    sample.running
                ),
                color: theme::TEXT,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
            Element::text(TextProps {
                content: format!("Load: {:.2} {:.2} {:.2}", load.one, load.five, load.fifteen),
                color: theme::TEXT,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
            Element::text(TextProps {
                content: format!("Uptime: {}", format::uptime(sample.uptime)),
                color: theme::MUTED,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
        ],
    )
}

/// How many terminal rows the header occupies, so the table knows how much
/// room is left.
pub fn height(cores: usize) -> u16 {
    let rows = rows_per_column(cores);
    let columns = cores.div_ceil(rows).max(1);
    let last_column_used = cores - rows * (columns - 1);
    // Memory and swap either tuck into the last column or add a column of
    // their own; either way the header is as tall as its tallest column.
    let meter_rows = if last_column_used + 2 <= rows {
        rows
    } else {
        rows.max(2)
    };
    (meter_rows + 1) as u16
}
